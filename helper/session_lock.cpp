/*
 * session_lock.cpp - GTK4 Session Lock implementation using gtk4-layer-shell
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "session_lock.hpp"

#include <utility>

namespace wayland {

SessionLock::SessionLock() = default;

SessionLock::~SessionLock() { cleanup(); }

SessionLock::SessionLock(SessionLock &&other) noexcept
    : instance_(std::exchange(other.instance_, nullptr)),
      state_(std::exchange(other.state_, LockState::Unlocked)),
      locked_callback_(std::move(other.locked_callback_)),
      failed_callback_(std::move(other.failed_callback_)),
      unlocked_callback_(std::move(other.unlocked_callback_)),
      monitor_callback_(std::move(other.monitor_callback_)),
      locked_handler_id_(std::exchange(other.locked_handler_id_, 0)),
      failed_handler_id_(std::exchange(other.failed_handler_id_, 0)),
      unlocked_handler_id_(std::exchange(other.unlocked_handler_id_, 0)),
      monitor_handler_id_(std::exchange(other.monitor_handler_id_, 0)) {}

auto SessionLock::operator=(SessionLock &&other) noexcept -> SessionLock & {
  if (this != &other) {
    cleanup();
    instance_ = std::exchange(other.instance_, nullptr);
    state_ = std::exchange(other.state_, LockState::Unlocked);
    locked_callback_ = std::move(other.locked_callback_);
    failed_callback_ = std::move(other.failed_callback_);
    unlocked_callback_ = std::move(other.unlocked_callback_);
    monitor_callback_ = std::move(other.monitor_callback_);
    locked_handler_id_ = std::exchange(other.locked_handler_id_, 0);
    failed_handler_id_ = std::exchange(other.failed_handler_id_, 0);
    unlocked_handler_id_ = std::exchange(other.unlocked_handler_id_, 0);
    monitor_handler_id_ = std::exchange(other.monitor_handler_id_, 0);
  }
  return *this;
}

void SessionLock::cleanup() {
  if (instance_) {
    // Disconnect signal handlers
    if (locked_handler_id_ != 0) {
      g_signal_handler_disconnect(instance_, locked_handler_id_);
      locked_handler_id_ = 0;
    }
    if (failed_handler_id_ != 0) {
      g_signal_handler_disconnect(instance_, failed_handler_id_);
      failed_handler_id_ = 0;
    }
    if (unlocked_handler_id_ != 0) {
      g_signal_handler_disconnect(instance_, unlocked_handler_id_);
      unlocked_handler_id_ = 0;
    }
    if (monitor_handler_id_ != 0) {
      g_signal_handler_disconnect(instance_, monitor_handler_id_);
      monitor_handler_id_ = 0;
    }

    // Unlock if still locked
    if (gtk_session_lock_instance_is_locked(instance_)) {
      gtk_session_lock_instance_unlock(instance_);
    }

    g_object_unref(instance_);
    instance_ = nullptr;
  }
  state_ = LockState::Unlocked;
}

auto SessionLock::is_supported() -> bool {
  return gtk_session_lock_is_supported() != FALSE;
}

auto SessionLock::lock() -> LockResult {
  if (!is_supported()) {
    return LockResult::NotSupported;
  }

  if (instance_ && gtk_session_lock_instance_is_locked(instance_)) {
    return LockResult::AlreadyLocked;
  }

  // Clean up any previous instance
  cleanup();

  // Create new instance
  instance_ = gtk_session_lock_instance_new();
  if (!instance_) {
    return LockResult::Failed;
  }

  // Connect signals
  locked_handler_id_ = g_signal_connect(instance_, "locked",
                                        G_CALLBACK(on_locked_signal), this);
  failed_handler_id_ = g_signal_connect(instance_, "failed",
                                        G_CALLBACK(on_failed_signal), this);
  unlocked_handler_id_ = g_signal_connect(instance_, "unlocked",
                                          G_CALLBACK(on_unlocked_signal), this);
  monitor_handler_id_ = g_signal_connect(instance_, "monitor",
                                         G_CALLBACK(on_monitor_signal), this);

  state_ = LockState::Locking;

  // Attempt to lock
  if (!gtk_session_lock_instance_lock(instance_)) {
    state_ = LockState::Failed;
    return LockResult::Failed;
  }

  return LockResult::Success;
}

void SessionLock::unlock() {
  if (instance_ && gtk_session_lock_instance_is_locked(instance_)) {
    gtk_session_lock_instance_unlock(instance_);
  }
}

// Signal handlers

void SessionLock::on_locked_signal(GtkSessionLockInstance * /*instance*/,
                                   gpointer data) {
  auto *self = static_cast<SessionLock *>(data);
  self->state_ = LockState::Locked;

  if (self->locked_callback_) {
    self->locked_callback_();
  }
}

void SessionLock::on_failed_signal(GtkSessionLockInstance * /*instance*/,
                                   gpointer data) {
  auto *self = static_cast<SessionLock *>(data);
  self->state_ = LockState::Failed;

  if (self->failed_callback_) {
    self->failed_callback_();
  }
}

void SessionLock::on_unlocked_signal(GtkSessionLockInstance * /*instance*/,
                                     gpointer data) {
  auto *self = static_cast<SessionLock *>(data);
  self->state_ = LockState::Unlocked;

  if (self->unlocked_callback_) {
    self->unlocked_callback_();
  }
}

void SessionLock::on_monitor_signal(GtkSessionLockInstance *instance,
                                    GdkMonitor *monitor, gpointer data) {
  auto *self = static_cast<SessionLock *>(data);

  if (self->monitor_callback_) {
    // Call the user's callback to create a window for this monitor
    GtkWindow *window = self->monitor_callback_(monitor);
    if (window) {
      // Assign the window to the monitor
      gtk_session_lock_instance_assign_window_to_monitor(instance, window,
                                                         monitor);
    }
  }
}

} // namespace wayland
