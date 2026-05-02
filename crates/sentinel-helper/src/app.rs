use crate::cli::Args;
use crate::i18n;
use cosmic::iced::advanced::layout::Limits;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, get_layer_surface,
};
use cosmic::iced::time::{self, Duration, Instant};
use cosmic::iced::{Background, Border, Color, Length, Shadow, Subscription, window};
use cosmic::widget::{button, column, container, progress_bar, row, scrollable, text};
use cosmic::{Action, Application, Element, Task, executor, theme};
use sentinel_config::Outcome;
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
    /// Resolved icon for the requesting executable, if the icon theme
    /// has a match for the basename. `None` means "no theme match —
    /// don't render an icon" rather than rendering a generic
    /// placeholder, which would just clutter the dialog.
    process_icon: Option<cosmic::widget::icon::Named>,
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
        let process_icon = resolve_process_icon(flags.process_exe.as_deref());
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
            process_icon,
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
        let keys = cosmic::iced::event::listen_with(|event, _status, _id| match event {
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

        // Resolve the three admin-overridable strings. Each one is
        // either:
        //   - a custom value the admin set in /etc/security/sentinel.conf
        //     (passed through verbatim — admins write whatever language),
        //   - or still equal to the built-in `sentinel-config` default,
        //     in which case we substitute the locale's translation so a
        //     fresh install actually feels localized.
        let title_text = self.localized_title();
        let message_text = self.localized_message();
        let secondary_text = self.localized_secondary();

        let mut content = column::with_capacity(12)
            .spacing(spacing.space_s)
            .align_x(Horizontal::Center)
            .push(text::title2(title_text));

        // Primary message — the "what's happening". Skipped if empty so
        // an admin who explicitly clears it doesn't get whitespace.
        if !message_text.is_empty() {
            content = content.push(text::body(message_text));
        }

        if let Some(exe) = self.args.process_exe.as_deref() {
            // Top of the card is the exe identification: optional icon
            // on the left (UAC-style — gives users a visual anchor for
            // what app they're authenticating), exe path on the right.
            let exe_text = text::monotext(truncate_for_display(exe, 280)).width(Length::Fill);
            let exe_row: Element<'_, Message> = match self.process_icon.clone() {
                Some(icon) => row::with_capacity(2)
                    .spacing(spacing.space_s)
                    .align_y(Vertical::Center)
                    .push(icon)
                    .push(exe_text)
                    .into(),
                None => exe_text.into(),
            };
            let mut info = column::with_capacity(8)
                .spacing(spacing.space_xxs)
                .width(Length::Fill)
                .push(exe_row);

            // Whether we have any expandable detail at all.
            let has_details = self.args.process_cmdline.is_some()
                || self.args.process_pid.is_some()
                || self.args.process_cwd.is_some()
                || self.args.requesting_user.is_some()
                || self.args.action.is_some();

            if has_details {
                let label = if self.show_details {
                    i18n::t("toggle-hide-details")
                } else {
                    i18n::t("toggle-show-details")
                };
                info = info.push(button::text(label).on_press(Message::ToggleDetails));

                if self.show_details {
                    let mut details = column::with_capacity(6)
                        .spacing(spacing.space_xxs)
                        .width(Length::Fill);
                    if let Some(cmdline) = self.args.process_cmdline.as_deref() {
                        details = details.push(detail_row(&i18n::t("detail-command"), cmdline));
                    }
                    if let Some(pid) = self.args.process_pid {
                        details =
                            details.push(detail_row(&i18n::t("detail-pid"), &pid.to_string()));
                    }
                    if let Some(cwd) = self.args.process_cwd.as_deref() {
                        details = details.push(detail_row(&i18n::t("detail-cwd"), cwd));
                    }
                    if let Some(user) = self.args.requesting_user.as_deref() {
                        details = details.push(detail_row(&i18n::t("detail-requested-by"), user));
                    }
                    if let Some(action) = self.args.action.as_deref() {
                        details = details.push(detail_row(&i18n::t("detail-action"), action));
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

        // Secondary instruction line — sits just above the buttons.
        if !secondary_text.is_empty() {
            content = content.push(text::caption(secondary_text));
        }

        if self.args.timeout > 0 {
            let frac = (self.elapsed_ms as f32) / ((self.args.timeout * 1000) as f32);
            let remaining = self.args.timeout.saturating_sub(self.elapsed_ms / 1000);
            content = content.push(progress_bar::determinate_linear(frac.min(1.0)));
            content = content.push(text::caption(i18n::t_int(
                "auto-deny-in",
                "seconds",
                remaining as i64,
            )));
        }

        let mut allow_btn = button::suggested(i18n::t("button-allow"));
        if self.allow_enabled {
            allow_btn = allow_btn.on_press(Message::Allow);
        }
        let deny_btn = button::destructive(i18n::t("button-deny")).on_press(Message::Deny);

        let buttons = if self.allow_first {
            row::with_capacity(2)
                .spacing(12)
                .push(allow_btn)
                .push(deny_btn)
        } else {
            row::with_capacity(2)
                .spacing(12)
                .push(deny_btn)
                .push(allow_btn)
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

    /// Title text. If the admin hasn't customized it (still equal to
    /// `sentinel_config::DEFAULT_TITLE`), render the locale's
    /// `dialog-title-default`. Otherwise pass the admin's value through.
    fn localized_title(&self) -> String {
        if self.args.title == sentinel_config::DEFAULT_TITLE {
            i18n::t("dialog-title-default")
        } else {
            self.args.title.clone()
        }
    }

    /// Same logic for the body message. The default template carries a
    /// `{$process}` placeholder which the i18n bundle expands; the
    /// process name comes from the basename of `--process-exe` (falling
    /// back to "unknown" so substitutions never produce empty tokens).
    ///
    /// Note: when comparing against `DEFAULT_MESSAGE` we look at the
    /// already-substituted form the caller passed us. The PAM module
    /// runs `format_message(&cfg.message, ...)` and sends the result —
    /// so we re-run the same substitution against `DEFAULT_MESSAGE` to
    /// get a comparable string.
    fn localized_message(&self) -> String {
        let proc_name = self.process_name_for_subst();
        let default_substituted = sentinel_config::format_message(
            sentinel_config::DEFAULT_MESSAGE,
            self.args.requesting_user.as_deref().unwrap_or(""),
            self.args.action.as_deref().unwrap_or(""),
            &proc_name,
        );
        if self.args.message == default_substituted {
            i18n::t_str("dialog-message-default", "process", &proc_name)
        } else {
            self.args.message.clone()
        }
    }

    /// Same logic for the secondary line. The default has no tokens, so
    /// no substitution dance.
    fn localized_secondary(&self) -> String {
        let secondary_substituted = sentinel_config::format_message(
            sentinel_config::DEFAULT_SECONDARY,
            self.args.requesting_user.as_deref().unwrap_or(""),
            self.args.action.as_deref().unwrap_or(""),
            &self.process_name_for_subst(),
        );
        if self.args.secondary == secondary_substituted {
            i18n::t("dialog-secondary-default")
        } else {
            self.args.secondary.clone()
        }
    }

    fn process_name_for_subst(&self) -> String {
        self.args
            .process_exe
            .as_deref()
            .and_then(sentinel_config::process_basename)
            .unwrap_or("unknown")
            .to_string()
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

/// Try to resolve an icon theme entry for the requesting executable's
/// basename (e.g. `/usr/bin/firefox` → `firefox`). Returns `None` when
/// the theme has no match — caller renders no icon in that case rather
/// than a generic placeholder.
///
/// The lookup is freedesktop-icons-spec via libcosmic's icon theme
/// machinery, which uses a file-based cache. Done once at app init so
/// repeated dialog renders don't walk the theme directory.
fn resolve_process_icon(process_exe: Option<&str>) -> Option<cosmic::widget::icon::Named> {
    let basename = process_exe.and_then(sentinel_config::process_basename)?;
    let named = cosmic::widget::icon::from_name(basename.to_string())
        .size(48)
        // Disable Named's default name-truncation fallback (which would
        // try `firefox-nightly` → `firefox-` → `firefox`). For our use
        // case "no exact match" should mean "no icon" — a misleading
        // partial match (e.g. `python3-foo` falling back to `python3`)
        // is worse than no icon at all.
        .fallback(None);
    if named.clone().path().is_some() {
        Some(named)
    } else {
        None
    }
}

/// Hard cap on rendered text length. Anything past `max_chars` is replaced
/// with an ellipsis so a malicious or accidental megabyte-long argv can't
/// stall the layout engine.
fn truncate_for_display(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push(' ');
    out.push_str(&i18n::t("truncated-suffix"));
    out
}
