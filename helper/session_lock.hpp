/*
 * session_lock.hpp - GTK4 Session Lock wrapper using gtk4-layer-shell
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once

#include <gtk/gtk.h>
#include <gtk4-layer-shell/gtk4-session-lock.h>

#include <functional>
#include <string_view>

namespace wayland {

// Session lock state
enum class LockState { Unlocked, Locking, Locked, Failed };

// Result of attempting to acquire a session lock
enum class LockResult {
  Success,
  NotSupported,
  AlreadyLocked,
  Failed,
};

// Callback types
using LockedCallback = std::function<void()>;
using FailedCallback = std::function<void()>;
using UnlockedCallback = std::function<void()>;
using MonitorCallback = std::function<GtkWindow *(GdkMonitor *)>;

// RAII wrapper for gtk4-session-lock
class SessionLock {
public:
  SessionLock();
  ~SessionLock();

  // Non-copyable, movable
  SessionLock(SessionLock const &) = delete;
  auto operator=(SessionLock const &) -> SessionLock & = delete;
  SessionLock(SessionLock &&other) noexcept;
  auto operator=(SessionLock &&other) noexcept -> SessionLock &;

  // Check if session lock is supported on this system
  [[nodiscard]] static auto is_supported() -> bool;

  // Acquire the session lock
  [[nodiscard]] auto lock() -> LockResult;

  // Unlock the session
  void unlock();

  // Get current lock state
  [[nodiscard]] auto state() const noexcept -> LockState { return state_; }

  // Check if locked
  [[nodiscard]] auto is_locked() const noexcept -> bool {
    return state_ == LockState::Locked;
  }

  // Set callbacks
  void on_locked(LockedCallback callback) {
    locked_callback_ = std::move(callback);
  }
  void on_failed(FailedCallback callback) {
    failed_callback_ = std::move(callback);
  }
  void on_unlocked(UnlockedCallback callback) {
    unlocked_callback_ = std::move(callback);
  }

  // Set the monitor callback - called for each monitor when lock is acquired
  // The callback should return a new GtkWindow to display on that monitor
  void on_monitor(MonitorCallback callback) {
    monitor_callback_ = std::move(callback);
  }

private:
  void cleanup();

  // Signal handlers
  static void on_locked_signal(GtkSessionLockInstance *instance, gpointer data);
  static void on_failed_signal(GtkSessionLockInstance *instance, gpointer data);
  static void on_unlocked_signal(GtkSessionLockInstance *instance,
                                 gpointer data);
  static void on_monitor_signal(GtkSessionLockInstance *instance,
                                GdkMonitor *monitor, gpointer data);

  GtkSessionLockInstance *instance_ = nullptr;
  LockState state_ = LockState::Unlocked;

  LockedCallback locked_callback_;
  FailedCallback failed_callback_;
  UnlockedCallback unlocked_callback_;
  MonitorCallback monitor_callback_;

  // Signal handler IDs for cleanup
  gulong locked_handler_id_ = 0;
  gulong failed_handler_id_ = 0;
  gulong unlocked_handler_id_ = 0;
  gulong monitor_handler_id_ = 0;
};

// Get a human-readable string for LockResult
[[nodiscard]] constexpr auto to_string(LockResult result) -> std::string_view {
  switch (result) {
  case LockResult::Success:
    return "Success";
  case LockResult::NotSupported:
    return "Session lock not supported by compositor";
  case LockResult::AlreadyLocked:
    return "Session is already locked";
  case LockResult::Failed:
    return "Failed to acquire session lock";
  }
  return "Unknown error";
}

} // namespace wayland
