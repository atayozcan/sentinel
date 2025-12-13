/*
 * window.hpp - Confirmation dialog window
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once

#include <adwaita.h>

#include <chrono>
#include <cstdint>
#include <functional>
#include <optional>
#include <string>
#include <string_view>

namespace confirm {

// Dialog result
enum class Result { Allow, Deny, Timeout };

// Convert result to string for output
[[nodiscard]] constexpr auto to_string(Result result) -> std::string_view {
  switch (result) {
  case Result::Allow:
    return "ALLOW";
  case Result::Deny:
    return "DENY";
  case Result::Timeout:
    return "TIMEOUT";
  }
  return "DENY";
}

// Parameters for the confirmation window
struct WindowParams {
  std::string_view title = "Authentication Required";
  std::string_view message =
      "An application is requesting elevated privileges.";
  std::string_view secondary = "Click Allow to continue or Deny to cancel.";
  std::optional<std::string_view> process_exe = std::nullopt;

  std::chrono::seconds timeout{30};
  std::chrono::milliseconds min_display_time{500};

  bool randomize = false;
};

// Result callback type
using ResultCallback = std::function<void(Result)>;

// GObject type declarations
G_BEGIN_DECLS

#define CONFIRM_TYPE_WINDOW (confirm_window_get_type())
G_DECLARE_FINAL_TYPE(ConfirmWindow, confirm_window, CONFIRM, WINDOW, AdwWindow)

// Create a new confirmation window
ConfirmWindow *confirm_window_new(AdwApplication *app,
                                  WindowParams const &params,
                                  ResultCallback on_result);

G_END_DECLS

} // namespace confirm
