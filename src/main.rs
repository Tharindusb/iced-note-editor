#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::futures::future::ok;
use iced::highlighter::{self, Highlighter};
use iced::theme;
use iced::widget::horizontal_space;
use iced::widget::row;
use iced::widget::{button, column, container, pick_list, text, text_editor, tooltip};
use iced::{executor, Length};
use iced::{Application, Element, Font, Settings, Theme};

use iced::keyboard;

fn main() -> iced::Result {
    Editor::run(Settings {
        default_font: Font::MONOSPACE,
        fonts: vec![include_bytes!("../res/fonts/editor-icons.ttf")
            .as_slice()
            .into()],
        ..Settings::default()
    })
}

struct Editor {
    path: Option<PathBuf>,
    content: text_editor::Content,
    error: Option<Error>,
    theme: highlighter::Theme,
    is_dirty: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(text_editor::Action),
    New,
    Open,
    Save,
    FileSaved(Result<PathBuf, Error>),
    FileOpened(Result<(PathBuf, Arc<String>), Error>),
    ThemeSelected(highlighter::Theme),
}

impl Application for Editor {
    type Message = Message;
    type Theme = Theme;
    type Flags = ();
    type Executor = executor::Default;

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Message>) {
        (
            Self {
                path: None,
                content: text_editor::Content::new(),
                error: None,
                theme: highlighter::Theme::SolarizedDark,
                is_dirty: true,
            },
            iced::Command::perform(load_file(default_file()), Message::FileOpened),
        )
    }

    fn title(&self) -> String {
        String::from("Editor")
    }

    fn update(&mut self, message: Message) -> iced::Command<Message> {
        match message {
            Message::Edit(action) => {
                self.is_dirty = self.is_dirty || action.is_edit();
                self.error = None;
                self.content.edit(action);

                iced::Command::none()
            }
            Message::New => {
                self.path = None;
                self.content = text_editor::Content::new();
                self.is_dirty = true;
                iced::Command::none()
            }
            Message::Save => {
                let text = self.content.text();
                iced::Command::perform(save_file(self.path.clone(), text), Message::FileSaved)
            }
            Message::FileSaved(Ok((path))) => {
                self.path = Some(path);
                self.is_dirty = false;
                iced::Command::none()
            }
            Message::FileSaved(Err(error)) => {
                self.error = Some(error);
                iced::Command::none()
            }
            Message::Open => iced::Command::perform(pick_file(), Message::FileOpened),
            Message::FileOpened(Ok((path, content))) => {
                self.path = Some(path);
                self.content = text_editor::Content::with(&content);
                self.is_dirty = false;
                iced::Command::none()
            }
            Message::FileOpened(Err(error)) => {
                self.error = Some(error);
                iced::Command::none()
            }
            Message::ThemeSelected(theme) => {
                self.theme = theme;
                iced::Command::none()
            }
        }
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        keyboard::on_key_press(|key_code, modifiers| match key_code {
            keyboard::KeyCode::S if modifiers.command() => Some(Message::Save),
            _ => None,
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let controls = row![
            action(new_icon(), "New file", Some(Message::New)),
            action(open_icon(), "Open file", Some(Message::Open)),
            action(
                save_icon(),
                "Save file",
                self.is_dirty.then_some(Message::Save)
            ),
            horizontal_space(Length::Fill),
            pick_list(
                highlighter::Theme::ALL,
                Some(self.theme),
                Message::ThemeSelected
            )
        ]
        .spacing(10);

        let input = text_editor(&self.content)
            .on_edit(Message::Edit)
            .highlight::<Highlighter>(
                highlighter::Settings {
                    theme: self.theme,
                    extension: self
                        .path
                        .as_ref()
                        .and_then(|path| path.extension()?.to_str())
                        .unwrap_or("rs")
                        .to_string(),
                },
                |highlight, _theme| highlight.to_format(),
            );

        let status_bar = {
            let status = if let Some(Error::IOFailed(error)) = self.error.as_ref() {
                text(error.to_string())
            } else {
                match self.path.as_deref().and_then(Path::to_str) {
                    Some(path) => text(path).size(14),
                    None => text("New file"),
                }
            };

            let position = {
                let (line, column) = self.content.cursor_position();
                text(format!("{}:{}", line + 1, column + 1))
            };

            row![status, horizontal_space(Length::Fill), position]
        };
        container(column![controls, input, status_bar].spacing(10))
            .padding(10)
            .into()
    }

    fn theme(&self) -> Theme {
        if self.theme.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

async fn pick_file() -> Result<(PathBuf, Arc<String>), Error> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Choose a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;
    load_file(handle.path().to_owned()).await
}

fn action<'a>(
    content: Element<'a, Message>,
    label: &str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let is_disabled = on_press.is_none();
    tooltip(
        button(container(content).width(30).center_x())
            .on_press_maybe(on_press)
            .padding([5, 10])
            .style(if is_disabled {
                theme::Button::Secondary
            } else {
                theme::Button::Primary
            }),
        label,
        tooltip::Position::FollowCursor,
    )
    .style(theme::Container::Box)
    .into()
}

fn new_icon<'a>() -> Element<'a, Message> {
    icon('\u{E800}')
}

fn save_icon<'a>() -> Element<'a, Message> {
    icon('\u{E801}')
}

fn open_icon<'a>() -> Element<'a, Message> {
    icon('\u{F115}')
}

fn icon<'a, Message>(codepoint: char) -> Element<'a, Message> {
    const ICON_FONT: Font = Font::with_name("editor-icons");
    text(codepoint).font(ICON_FONT).into()
}

fn default_file() -> PathBuf {
    PathBuf::from(format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR")))
}

async fn load_file(path: PathBuf) -> Result<(PathBuf, Arc<String>), Error> {
    let contents = tokio::fs::read_to_string(&path)
        .await
        .map(Arc::new)
        .map_err(|e| e.kind())
        .map_err(Error::IOFailed)?;
    Ok((path, contents))
}

async fn save_file(path: Option<PathBuf>, text: String) -> Result<PathBuf, Error> {
    let path = if let Some(path) = path {
        path
    } else {
        rfd::AsyncFileDialog::new()
            .set_title("Choose a file name ..")
            .save_file()
            .await
            .ok_or(Error::DialogClosed)
            .map(|handle| handle.path().to_owned())?
    };

    tokio::fs::write(&path, text)
        .await
        .map_err(|e| Error::IOFailed(e.kind()));
    Ok(path)
}

#[derive(Debug, Clone)]
enum Error {
    DialogClosed,
    IOFailed(io::ErrorKind),
}
