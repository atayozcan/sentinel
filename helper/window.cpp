/*
 * window.cpp - Confirmation dialog window implementation
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "window.hpp"
#include "gtk_utils.hpp"

#include <gdk/wayland/gdkwayland.h>

#include <chrono>
#include <cstdlib>
#include <ctime>
#include <format>
#include <random>

namespace confirm {

namespace {

// CSS styles for the dialog
constexpr std::string_view kStylesCss = R"(
.confirm-window {
  background: transparent;
}
.session-lock-bg {
  background: alpha(#1a1a1a, 0.9);
}
.confirm-dialog {
  background: @card_bg_color;
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  min-width: 400px;
  padding: 32px 40px;
}
.confirm-title {
  font-size: 18pt;
  font-weight: bold;
}
.confirm-message {
  font-size: 11pt;
}
.confirm-secondary {
  font-size: 10pt;
  color: @dim_fg_color;
}
.confirm-process-box {
  background: alpha(@shade_color, 0.5);
  padding: 12px 16px;
  border-radius: 8px;
  margin-top: 8px;
}
.confirm-app-icon {
  opacity: 0.9;
}
.confirm-app-name {
  font-size: 11pt;
  font-weight: bold;
}
.confirm-process {
  font-size: 9pt;
  font-family: monospace;
  color: @dim_fg_color;
}
.timer-label {
  font-size: 9pt;
  color: @dim_fg_color;
}
.allow-button {
  background: @success_bg_color;
  color: @success_fg_color;
}
.deny-button {
  background: @error_bg_color;
  color: @error_fg_color;
}
)";

void setup_styles() {
  static bool initialized = false;
  if (initialized) {
    return;
  }
  initialized = true;

  auto *provider = gtk_css_provider_new();
  gtk_css_provider_load_from_string(provider, kStylesCss.data());
  gtk_style_context_add_provider_for_display(
      gdk_display_get_default(), GTK_STYLE_PROVIDER(provider),
      GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
  g_object_unref(provider);
}

// Random bool generator
[[nodiscard]] auto random_bool() -> bool {
  static std::random_device rd;
  static std::mt19937 gen(rd());
  static std::uniform_int_distribution<> dis(0, 1);
  return dis(gen) != 0;
}

} // namespace

// Private implementation struct
struct _ConfirmWindow {
  AdwWindow parent;

  // Widgets
  GtkWidget *main_box;
  GtkWidget *title_label;
  GtkWidget *message_label;
  GtkWidget *secondary_label;
  GtkWidget *process_label;
  GtkWidget *timer_label;
  GtkWidget *button_box;
  GtkWidget *allow_button;
  GtkWidget *deny_button;
  GtkWidget *progress_bar;

  // State
  std::int64_t start_time;
  guint timer_id;
  bool allow_enabled;
  bool result_sent;

  // Parameters (stored as copies)
  std::string title;
  std::string message;
  std::string secondary;
  std::string process_exe;
  std::chrono::seconds timeout;
  std::chrono::milliseconds min_display_time;
  bool randomize;

  // Result callback
  ResultCallback on_result;
};

G_DEFINE_TYPE(ConfirmWindow, confirm_window, ADW_TYPE_WINDOW)

namespace {

void send_result(ConfirmWindow *self, Result result) {
  if (self->result_sent) {
    return;
  }
  self->result_sent = true;

  if (self->on_result) {
    self->on_result(result);
  }

  gtk_window_close(GTK_WINDOW(self));
}

void on_allow_clicked(GtkButton * /*button*/, ConfirmWindow *self) {
  if (!self->allow_enabled) {
    return;
  }

  // Check minimum display time
  auto const now = g_get_monotonic_time();
  auto const elapsed_ms = (now - self->start_time) / 1000;

  if (elapsed_ms < self->min_display_time.count()) {
    // Too fast - possible automation attempt
    return;
  }

  send_result(self, Result::Allow);
}

void on_deny_clicked(GtkButton * /*button*/, ConfirmWindow *self) {
  send_result(self, Result::Deny);
}

auto on_timer_tick(ConfirmWindow *self) -> gboolean {
  auto const now = g_get_monotonic_time();
  auto const elapsed_ms = (now - self->start_time) / 1000;
  auto const elapsed_sec = elapsed_ms / 1000;
  auto const remaining =
      static_cast<std::int64_t>(self->timeout.count()) - elapsed_sec;

  if (remaining <= 0) {
    send_result(self, Result::Timeout);
    return G_SOURCE_REMOVE;
  }

  // Update timer label
  auto const timer_text = std::format("Auto-deny in {} seconds", remaining);
  gtk_label_set_text(GTK_LABEL(self->timer_label), timer_text.c_str());

  // Update progress bar
  auto const progress =
      static_cast<double>(elapsed_sec) /
      static_cast<double>(self->timeout.count());
  gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(self->progress_bar), progress);

  // Enable allow button after minimum time
  if (!self->allow_enabled &&
      elapsed_ms >= self->min_display_time.count()) {
    self->allow_enabled = true;
    gtk_widget_set_sensitive(self->allow_button, TRUE);
    gtk_widget_remove_css_class(self->allow_button, "dim-label");
  }

  return G_SOURCE_CONTINUE;
}

auto on_key_pressed(GtkEventControllerKey * /*controller*/, guint keyval,
                    guint /*keycode*/, GdkModifierType /*state*/,
                    ConfirmWindow *self) -> gboolean {
  // Escape to deny
  if (keyval == GDK_KEY_Escape) {
    send_result(self, Result::Deny);
    return TRUE;
  }

  return FALSE;
}

auto on_close_request(GtkWindow *window) -> gboolean {
  auto *self = CONFIRM_WINDOW(window);

  if (!self->result_sent) {
    send_result(self, Result::Deny);
  }

  return FALSE;
}

} // namespace

static void confirm_window_dispose(GObject *object) {
  auto *self = CONFIRM_WINDOW(object);

  if (self->timer_id != 0) {
    g_source_remove(self->timer_id);
    self->timer_id = 0;
  }

  G_OBJECT_CLASS(confirm_window_parent_class)->dispose(object);
}

static void confirm_window_finalize(GObject *object) {
  auto *self = CONFIRM_WINDOW(object);

  // Explicitly destroy std::string members
  self->title.~basic_string();
  self->message.~basic_string();
  self->secondary.~basic_string();
  self->process_exe.~basic_string();
  self->on_result.~function();

  G_OBJECT_CLASS(confirm_window_parent_class)->finalize(object);
}

static void confirm_window_class_init(ConfirmWindowClass *klass) {
  auto *object_class = G_OBJECT_CLASS(klass);
  object_class->dispose = confirm_window_dispose;
  object_class->finalize = confirm_window_finalize;
}

static void confirm_window_init(ConfirmWindow *self) {
  // Placement new for C++ members
  new (&self->title) std::string();
  new (&self->message) std::string();
  new (&self->secondary) std::string();
  new (&self->process_exe) std::string();
  new (&self->on_result) ResultCallback();

  self->allow_enabled = false;
  self->result_sent = false;
  self->timer_id = 0;
}

ConfirmWindow *confirm_window_new(AdwApplication *app,
                                  WindowParams const &params,
                                  ResultCallback on_result) {
  auto *self = static_cast<ConfirmWindow *>(
      g_object_new(CONFIRM_TYPE_WINDOW, "application", app, nullptr));

  // Copy parameters
  self->title = std::string(params.title);
  self->message = std::string(params.message);
  self->secondary = std::string(params.secondary);
  if (params.process_exe) {
    self->process_exe = std::string(*params.process_exe);
  }
  self->timeout = params.timeout;
  self->min_display_time = params.min_display_time;
  self->randomize = params.randomize;
  self->on_result = std::move(on_result);

  setup_styles();

  // Window properties
  // Note: resizable must be TRUE for session lock to properly fill the screen
  gtk_window_set_title(GTK_WINDOW(self), self->title.c_str());
  gtk_window_set_default_size(GTK_WINDOW(self), 450, 400);
  gtk_window_set_resizable(GTK_WINDOW(self), TRUE);
  gtk_window_set_modal(GTK_WINDOW(self), TRUE);
  gtk_window_set_deletable(GTK_WINDOW(self), FALSE);
  gtk_window_set_decorated(GTK_WINDOW(self), FALSE);

  // Connect close handler
  g_signal_connect(self, "close-request", G_CALLBACK(on_close_request),
                   nullptr);

  // Add keyboard controller
  auto *key_controller = gtk_event_controller_key_new();
  g_signal_connect(key_controller, "key-pressed", G_CALLBACK(on_key_pressed),
                   self);
  gtk_widget_add_controller(GTK_WIDGET(self), key_controller);

  // Overlay for centering content on fullscreen (session lock)
  auto *overlay = gtk_overlay_new();
  gtk_widget_set_hexpand(overlay, TRUE);
  gtk_widget_set_vexpand(overlay, TRUE);
  adw_window_set_content(ADW_WINDOW(self), overlay);

  // Background that fills the screen
  auto *background = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
  gtk_widget_set_hexpand(background, TRUE);
  gtk_widget_set_vexpand(background, TRUE);
  gtk_widget_add_css_class(background, "session-lock-bg");
  gtk_overlay_set_child(GTK_OVERLAY(overlay), background);

  // Main content box - centered via overlay
  self->main_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 16);
  gtk_widget_set_halign(self->main_box, GTK_ALIGN_CENTER);
  gtk_widget_set_valign(self->main_box, GTK_ALIGN_CENTER);
  gtk_widget_add_css_class(self->main_box, "card");
  gtk_widget_add_css_class(self->main_box, "confirm-dialog");
  gtk_overlay_add_overlay(GTK_OVERLAY(overlay), self->main_box);

  // Title
  self->title_label = gtk_label_new(self->title.c_str());
  gtk_widget_add_css_class(self->title_label, "confirm-title");
  gtk_label_set_wrap(GTK_LABEL(self->title_label), TRUE);
  gtk_label_set_justify(GTK_LABEL(self->title_label), GTK_JUSTIFY_CENTER);
  gtk_box_append(GTK_BOX(self->main_box), self->title_label);

  // Message
  self->message_label = gtk_label_new(self->message.c_str());
  gtk_widget_add_css_class(self->message_label, "confirm-message");
  gtk_label_set_wrap(GTK_LABEL(self->message_label), TRUE);
  gtk_label_set_justify(GTK_LABEL(self->message_label), GTK_JUSTIFY_CENTER);
  gtk_box_append(GTK_BOX(self->main_box), self->message_label);

  // Secondary message
  if (!self->secondary.empty()) {
    self->secondary_label = gtk_label_new(self->secondary.c_str());
    gtk_widget_add_css_class(self->secondary_label, "confirm-secondary");
    gtk_label_set_wrap(GTK_LABEL(self->secondary_label), TRUE);
    gtk_label_set_justify(GTK_LABEL(self->secondary_label), GTK_JUSTIFY_CENTER);
    gtk_box_append(GTK_BOX(self->main_box), self->secondary_label);
  }

  // Process info box with icon and details
  if (!self->process_exe.empty()) {
    auto *info_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 16);
    gtk_widget_add_css_class(info_box, "confirm-process-box");
    gtk_widget_set_halign(info_box, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(self->main_box), info_box);

    // Extract app name from path
    std::string_view exe_path = self->process_exe;
    auto const last_slash = exe_path.rfind('/');
    std::string_view app_name =
        (last_slash != std::string_view::npos) ? exe_path.substr(last_slash + 1) : exe_path;

    // Try to find an icon for the app
    auto *icon_theme = gtk_icon_theme_get_for_display(gdk_display_get_default());
    auto *app_icon = gtk_image_new();
    gtk_image_set_pixel_size(GTK_IMAGE(app_icon), 48);

    // Try app name as icon, fallback to generic application icon
    if (gtk_icon_theme_has_icon(icon_theme, std::string(app_name).c_str())) {
      gtk_image_set_from_icon_name(GTK_IMAGE(app_icon), std::string(app_name).c_str());
    } else {
      gtk_image_set_from_icon_name(GTK_IMAGE(app_icon), "application-x-executable");
    }
    gtk_widget_add_css_class(app_icon, "confirm-app-icon");
    gtk_box_append(GTK_BOX(info_box), app_icon);

    // Details box (name + path)
    auto *details_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
    gtk_widget_set_valign(details_box, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(info_box), details_box);

    // App name label
    auto *name_label = gtk_label_new(std::string(app_name).c_str());
    gtk_widget_add_css_class(name_label, "confirm-app-name");
    gtk_label_set_xalign(GTK_LABEL(name_label), 0);
    gtk_box_append(GTK_BOX(details_box), name_label);

    // Full path label
    self->process_label = gtk_label_new(self->process_exe.c_str());
    gtk_widget_add_css_class(self->process_label, "confirm-process");
    gtk_label_set_wrap(GTK_LABEL(self->process_label), TRUE);
    gtk_label_set_wrap_mode(GTK_LABEL(self->process_label), PANGO_WRAP_CHAR);
    gtk_label_set_selectable(GTK_LABEL(self->process_label), TRUE);
    gtk_label_set_xalign(GTK_LABEL(self->process_label), 0);
    gtk_box_append(GTK_BOX(details_box), self->process_label);
  }

  // Progress bar
  self->progress_bar = gtk_progress_bar_new();
  gtk_widget_set_margin_top(self->progress_bar, 8);
  gtk_box_append(GTK_BOX(self->main_box), self->progress_bar);

  // Timer label
  auto const timer_text =
      std::format("Auto-deny in {} seconds", self->timeout.count());
  self->timer_label = gtk_label_new(timer_text.c_str());
  gtk_widget_add_css_class(self->timer_label, "timer-label");
  gtk_box_append(GTK_BOX(self->main_box), self->timer_label);

  // Button box
  self->button_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);
  gtk_widget_set_halign(self->button_box, GTK_ALIGN_CENTER);
  gtk_widget_set_margin_top(self->button_box, 8);
  gtk_box_append(GTK_BOX(self->main_box), self->button_box);

  // Create buttons
  self->deny_button = gtk_button_new_with_label("Deny");
  gtk_widget_add_css_class(self->deny_button, "deny-button");
  gtk_widget_add_css_class(self->deny_button, "pill");
  gtk_widget_set_size_request(self->deny_button, 120, -1);
  g_signal_connect(self->deny_button, "clicked", G_CALLBACK(on_deny_clicked),
                   self);

  self->allow_button = gtk_button_new_with_label("Allow");
  gtk_widget_add_css_class(self->allow_button, "allow-button");
  gtk_widget_add_css_class(self->allow_button, "pill");
  gtk_widget_add_css_class(self->allow_button, "suggested-action");
  gtk_widget_set_size_request(self->allow_button, 120, -1);
  gtk_widget_set_sensitive(self->allow_button, FALSE);
  gtk_widget_add_css_class(self->allow_button, "dim-label");
  g_signal_connect(self->allow_button, "clicked", G_CALLBACK(on_allow_clicked),
                   self);

  // Add buttons - randomize order if requested
  if (self->randomize && random_bool()) {
    gtk_box_append(GTK_BOX(self->button_box), self->allow_button);
    gtk_box_append(GTK_BOX(self->button_box), self->deny_button);
  } else {
    gtk_box_append(GTK_BOX(self->button_box), self->deny_button);
    gtk_box_append(GTK_BOX(self->button_box), self->allow_button);
  }

  // Record start time
  self->start_time = g_get_monotonic_time();

  // Start timeout timer
  if (self->timeout.count() > 0) {
    self->timer_id =
        g_timeout_add(100, reinterpret_cast<GSourceFunc>(on_timer_tick), self);
  } else {
    gtk_widget_set_visible(self->timer_label, FALSE);
    gtk_widget_set_visible(self->progress_bar, FALSE);
    self->allow_enabled = true;
    gtk_widget_set_sensitive(self->allow_button, TRUE);
    gtk_widget_remove_css_class(self->allow_button, "dim-label");
  }

  return self;
}

} // namespace confirm
