/*
 * main.cpp - PAM confirm helper main entry point
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "gtk_utils.hpp"
#include "session_lock.hpp"
#include "window.hpp"

#include <chrono>
#include <clocale>
#include <cstdlib>
#include <cstring>
#include <format>
#include <optional>
#include <print>
#include <string>
#include <string_view>

namespace {

struct AppConfig {
  confirm::WindowParams params;
  bool use_session_lock = true;
};

void print_usage(std::string_view prog) {
  std::println(stderr, "Usage: {} [OPTIONS]", prog);
  std::println(stderr, "");
  std::println(stderr, "Options:");
  std::println(stderr, "  --title TEXT          Dialog title");
  std::println(stderr, "  --message TEXT        Primary message");
  std::println(stderr, "  --secondary TEXT      Secondary message");
  std::println(stderr, "  --process-exe PATH    Path to requesting process");
  std::println(stderr,
               "  --timeout SECONDS     Auto-deny timeout (0 = no timeout)");
  std::println(stderr,
               "  --min-time MS         Minimum display time in milliseconds");
  std::println(stderr, "  --randomize           Randomize button positions");
  std::println(stderr, "  --no-session-lock     Don't use Wayland session lock");
  std::println(stderr, "  --help                Show this help");
  std::println(stderr, "");
  std::println(stderr, "Output:");
  std::println(stderr, "  Prints ALLOW, DENY, or TIMEOUT to stdout");
}

[[nodiscard]] auto parse_args(int argc, char *argv[])
    -> std::optional<AppConfig> {
  AppConfig config;

  for (int i = 1; i < argc; ++i) {
    std::string_view const arg = argv[i];

    if (arg == "--help" || arg == "-h") {
      print_usage(argv[0]);
      return std::nullopt;
    }

    if (arg == "--title" && i + 1 < argc) {
      config.params.title = argv[++i];
    } else if (arg == "--message" && i + 1 < argc) {
      config.params.message = argv[++i];
    } else if (arg == "--secondary" && i + 1 < argc) {
      config.params.secondary = argv[++i];
    } else if (arg == "--process-exe" && i + 1 < argc) {
      config.params.process_exe = argv[++i];
    } else if (arg == "--timeout" && i + 1 < argc) {
      config.params.timeout = std::chrono::seconds(std::atoi(argv[++i]));
    } else if (arg == "--min-time" && i + 1 < argc) {
      config.params.min_display_time =
          std::chrono::milliseconds(std::atoi(argv[++i]));
    } else if (arg == "--randomize") {
      config.params.randomize = true;
    } else if (arg == "--no-session-lock") {
      config.use_session_lock = false;
    }
  }

  return config;
}

// Global state for the application
struct AppState {
  AppConfig config;
  confirm::Result result = confirm::Result::Deny;
  wayland::SessionLock session_lock;
  AdwApplication *app = nullptr;
  bool first_monitor = true;
};

// Handle result from confirmation window
void on_result(AppState *state, confirm::Result result) {
  state->result = result;

  // Unlock session before quitting
  if (state->session_lock.is_locked()) {
    state->session_lock.unlock();
  }

  if (state->app) {
    g_application_quit(G_APPLICATION(state->app));
  }
}

void on_activate_with_lock(AdwApplication *app, AppState *state) {
  state->app = app;

  // Set up session lock callbacks
  state->session_lock.on_locked([]() {
    // Session is now locked - windows will be shown via monitor callback
  });

  state->session_lock.on_failed([state]() {
    std::println(stderr, "Error: Failed to acquire session lock");
    state->result = confirm::Result::Deny;
    g_application_quit(G_APPLICATION(state->app));
  });

  state->session_lock.on_unlocked([state]() {
    // Session unlocked - quit the application
    g_application_quit(G_APPLICATION(state->app));
  });

  // Create a window for each monitor
  state->session_lock.on_monitor([state](GdkMonitor *monitor) -> GtkWindow * {
    // Only show the dialog on the first monitor
    // Other monitors get a blank window
    if (state->first_monitor) {
      state->first_monitor = false;

      auto *window = confirm::confirm_window_new(
          state->app, state->config.params,
          [state](confirm::Result result) { on_result(state, result); });

      return GTK_WINDOW(window);
    }

    // For additional monitors, create a simple blank window
    auto *window = gtk_window_new();
    gtk_window_set_decorated(GTK_WINDOW(window), FALSE);

    // Add a dark background
    auto *provider = gtk_css_provider_new();
    gtk_css_provider_load_from_string(provider,
                                      "window { background: #1a1a1a; }");
    gtk_style_context_add_provider_for_display(
        gdk_monitor_get_display(monitor), GTK_STYLE_PROVIDER(provider),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    g_object_unref(provider);

    return GTK_WINDOW(window);
  });

  // Attempt to lock
  auto const result = state->session_lock.lock();
  if (result != wayland::LockResult::Success) {
    std::println(stderr, "Error: {}", wayland::to_string(result));
    state->result = confirm::Result::Deny;
    g_application_quit(G_APPLICATION(app));
  }
}

void on_activate_without_lock(AdwApplication *app, AppState *state) {
  state->app = app;

  // Create and show the confirmation window normally
  auto *window = confirm::confirm_window_new(
      app, state->config.params,
      [state](confirm::Result result) { on_result(state, result); });

  gtk_window_present(GTK_WINDOW(window));
}

void on_activate(AdwApplication *app, AppState *state) {
  if (state->config.use_session_lock &&
      wayland::SessionLock::is_supported()) {
    on_activate_with_lock(app, state);
  } else {
    if (state->config.use_session_lock) {
      std::println(stderr,
                   "Warning: Session lock not supported, running without it");
    }
    on_activate_without_lock(app, state);
  }
}

} // namespace

auto main(int argc, char *argv[]) -> int {
  // Parse arguments before GTK init
  auto const config = parse_args(argc, argv);
  if (!config) {
    return EXIT_SUCCESS; // --help was shown
  }

  std::setlocale(LC_ALL, "");

  // Check for Wayland display
  if (auto const *wayland_display = std::getenv("WAYLAND_DISPLAY");
      !wayland_display || *wayland_display == '\0') {
    std::println(stderr, "Error: No Wayland display available");
    std::println(stderr, "This application requires a Wayland compositor");
    return EXIT_FAILURE;
  }

  AppState state{
      .config = *config,
      .result = confirm::Result::Deny,
      .session_lock = {},
      .app = nullptr,
      .first_monitor = true,
  };

  // Create application
  auto *app = adw_application_new("com.github.sentinel.helper",
                                  G_APPLICATION_DEFAULT_FLAGS);

  g_signal_connect(app, "activate", G_CALLBACK(on_activate), &state);

  // Run - pass 0 args since we already parsed them
  static_cast<void>(g_application_run(G_APPLICATION(app), 0, nullptr));

  g_object_unref(app);

  // Output result
  std::println("{}", confirm::to_string(state.result));

  return state.result == confirm::Result::Allow ? EXIT_SUCCESS : EXIT_FAILURE;
}
