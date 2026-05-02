use crate::cli::Args;
use crate::result::Outcome;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, get_layer_surface,
};
use cosmic::iced::time::{self, Duration, Instant};
use cosmic::iced::{Background, Border, Color, Length, Shadow, Subscription, window};
use cosmic::iced::advanced::layout::Limits;
use cosmic::widget::{button, column, container, progress_bar, row, scrollable, text};
use cosmic::{Action, Application, Element, Task, executor, theme};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

/// Process-wide outcome cell. The application writes here on close so that
/// `main` can output the final result after `cosmic::app::run` returns.
pub static OUTCOME: AtomicI32 = AtomicI32::new(-1);

pub fn store_outcome(outcome: Outcome) {
    let v = match outcome {
        Outcome::Allow => 0,
        Outcome::Deny => 1,
        Outcome::Timeout => 2,
    };
    OUTCOME.store(v, Ordering::SeqCst);
}

pub fn loaded_outcome() -> Outcome {
    match OUTCOME.load(Ordering::SeqCst) {
        0 => Outcome::Allow,
        2 => Outcome::Timeout,
        _ => Outcome::Deny,
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
    Allow,
    Deny,
    Escape,
    ToggleDetails,
}

pub struct ConfirmApp {
    core: cosmic::app::Core,
    args: Arc<Args>,
    started: Instant,
    elapsed_ms: u64,
    allow_first: bool,
    allow_enabled: bool,
    finished: bool,
    show_details: bool,
    surface_id: Option<window::Id>,
}

impl Application for ConfirmApp {
    type Executor = executor::Default;
    type Flags = Arc<Args>;
    type Message = Message;

    const APP_ID: &'static str = "com.github.sentinel.helper";

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn init(core: cosmic::app::Core, flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let allow_first = flags.randomize && rand::random_bool(0.5);
        let windowed = flags.windowed;
        let mut app = Self {
            core,
            args: flags,
            started: Instant::now(),
            elapsed_ms: 0,
            allow_first,
            allow_enabled: false,
            finished: false,
            show_details: false,
            surface_id: None,
        };

        let task = if windowed {
            Task::none()
        } else {
            let id = window::Id::unique();
            app.surface_id = Some(id);
            let settings = SctkLayerSurfaceSettings {
                id,
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                namespace: "sentinel".into(),
                exclusive_zone: -1,
                size: Some((None, None)),
                size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                ..Default::default()
            };
            get_layer_surface::<Action<Self::Message>>(settings)
        };

        (app, task)
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if self.finished {
            return Subscription::none();
        }
        use cosmic::iced::keyboard::{Event as KeyEvent, Key, key::Named};

        let tick = time::every(Duration::from_millis(100)).map(Message::Tick);
        let keys =
            cosmic::iced::event::listen_with(|event, _status, _id| match event {
                cosmic::iced::Event::Keyboard(KeyEvent::KeyPressed {
                    key: Key::Named(Named::Escape),
                    ..
                }) => Some(Message::Escape),
                _ => None,
            });
        Subscription::batch([tick, keys])
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::Tick(_now) => {
                if self.finished {
                    return Task::none();
                }
                let elapsed = self.started.elapsed();
                self.elapsed_ms = elapsed.as_millis() as u64;

                if !self.allow_enabled && self.elapsed_ms >= self.args.min_time {
                    self.allow_enabled = true;
                }

                if self.args.timeout > 0 {
                    let timeout_ms = self.args.timeout * 1000;
                    if self.elapsed_ms >= timeout_ms {
                        self.finished = true;
                        store_outcome(Outcome::Timeout);
                        return cosmic::iced::exit();
                    }
                }
                Task::none()
            }
            Message::Allow => {
                if !self.allow_enabled || self.finished {
                    return Task::none();
                }
                self.finished = true;
                store_outcome(Outcome::Allow);
                cosmic::iced::exit()
            }
            Message::Deny | Message::Escape => {
                if self.finished {
                    return Task::none();
                }
                self.finished = true;
                store_outcome(Outcome::Deny);
                cosmic::iced::exit()
            }
            Message::ToggleDetails => {
                self.show_details = !self.show_details;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.dialog_view()
    }

    fn view_window(&self, _id: window::Id) -> Element<'_, Self::Message> {
        // Layer-shell surface uses this entry point under the multi-window/daemon path.
        self.dialog_view()
    }
}

impl ConfirmApp {
    fn dialog_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;

        let mut content = column::with_capacity(10)
            .spacing(spacing.space_s)
            .align_x(Horizontal::Center)
            .push(text::title2(self.args.title.clone()));

        if let Some(exe) = self.args.process_exe.as_deref() {
            let mut info = column::with_capacity(8)
                .spacing(spacing.space_xxs)
                .width(Length::Fill)
                .push(text::monotext(truncate_for_display(exe, 280)).width(Length::Fill));

            // Whether we have any expandable detail at all.
            let has_details = self.args.process_cmdline.is_some()
                || self.args.process_pid.is_some()
                || self.args.process_cwd.is_some()
                || self.args.requesting_user.is_some()
                || self.args.action.is_some();

            if has_details {
                let label = if self.show_details {
                    "▾ Hide details"
                } else {
                    "▸ Show details"
                };
                info = info.push(button::text(label).on_press(Message::ToggleDetails));

                if self.show_details {
                    let mut details = column::with_capacity(6)
                        .spacing(spacing.space_xxs)
                        .width(Length::Fill);
                    if let Some(cmdline) = self.args.process_cmdline.as_deref() {
                        details = details.push(detail_row("Command", cmdline));
                    }
                    if let Some(pid) = self.args.process_pid {
                        details = details.push(detail_row("PID", &pid.to_string()));
                    }
                    if let Some(cwd) = self.args.process_cwd.as_deref() {
                        details = details.push(detail_row("Working dir", cwd));
                    }
                    if let Some(user) = self.args.requesting_user.as_deref() {
                        details = details.push(detail_row("Requested by", user));
                    }
                    if let Some(action) = self.args.action.as_deref() {
                        details = details.push(detail_row("Action", action));
                    }
                    // Cap the expanded area at ~220px; long fields scroll.
                    info = info.push(
                        scrollable(details)
                            .height(Length::Shrink)
                            .width(Length::Fill),
                    );
                }
            }

            content = content.push(container(info).class(theme::Container::Card).padding(12));
        }

        if self.args.timeout > 0 {
            let frac = (self.elapsed_ms as f32) / ((self.args.timeout * 1000) as f32);
            let remaining = self.args.timeout.saturating_sub(self.elapsed_ms / 1000);
            content = content.push(progress_bar::determinate_linear(frac.min(1.0)));
            content = content.push(text::caption(format!("Auto-deny in {remaining}s")));
        }

        let mut allow_btn = button::suggested("Allow");
        if self.allow_enabled {
            allow_btn = allow_btn.on_press(Message::Allow);
        }
        let deny_btn = button::destructive("Deny").on_press(Message::Deny);

        let buttons = if self.allow_first {
            row::with_capacity(2).spacing(12).push(allow_btn).push(deny_btn)
        } else {
            row::with_capacity(2).spacing(12).push(deny_btn).push(allow_btn)
        };
        content = content.push(buttons);

        // Dialog card. max_width keeps the box readable on ultrawides;
        // max_height keeps a long expanded cmdline from pushing buttons
        // off-screen on small displays. Inner `scrollable` handles overflow.
        let card = container(scrollable(content).width(Length::Shrink))
            .padding(32)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .max_width(520.0)
            .max_height(640.0)
            .class(theme::Container::custom(|theme| {
                let cosmic = theme.cosmic();
                cosmic::iced::widget::container::Style {
                    text_color: Some(cosmic.background.on.into()),
                    background: Some(Background::Color(cosmic.background.base.into())),
                    border: Border {
                        radius: cosmic.radius_m().into(),
                        width: 1.0,
                        color: cosmic.background.divider.into(),
                    },
                    shadow: Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.45),
                        offset: cosmic::iced::Vector::new(0.0, 8.0),
                        blur_radius: 24.0,
                    },
                    icon_color: None,
                    snap: true,
                }
            }));

        // Translucent backdrop covering the entire output. In layer-shell mode
        // this is the full screen; in --windowed mode it just fills the window.
        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .class(theme::Container::custom(|_theme| {
                cosmic::iced::widget::container::Style {
                    text_color: None,
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55))),
                    border: Border::default(),
                    shadow: Shadow::default(),
                    icon_color: None,
                    snap: true,
                }
            }))
            .into()
    }
}

/// One labelled row in the expanded details section. The value is
/// width-bounded and length-clipped so a 100k-byte argv can't blow up
/// layout or memory.
fn detail_row<'a>(label: &str, value: &str) -> Element<'a, Message> {
    column::with_capacity(2)
        .width(Length::Fill)
        .push(text::caption(format!("{label}:")))
        .push(text::monotext(truncate_for_display(value, 4096)).width(Length::Fill))
        .into()
}

/// Hard cap on rendered text length. Anything past `max_chars` is replaced
/// with an ellipsis so a malicious or accidental megabyte-long argv can't
/// stall the layout engine.
fn truncate_for_display(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str(" … [truncated]");
    out
}
