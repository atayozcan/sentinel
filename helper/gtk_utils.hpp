/*
 * gtk_utils.hpp - Modern C++ RAII wrappers for GTK4/GObject
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once

#include <adwaita.h>
#include <gtk/gtk.h>

#include <concepts>
#include <functional>
#include <memory>
#include <string_view>
#include <utility>

namespace gtk {

// Concept for GObject-derived types
template <typename T>
concept GObjectDerived = requires(T *ptr) {
  { G_IS_OBJECT(ptr) } -> std::convertible_to<bool>;
};

// Custom deleter for GObject types
struct GObjectDeleter {
  void operator()(gpointer obj) const noexcept {
    if (obj) {
      g_object_unref(obj);
    }
  }
};

// Smart pointer alias for GObject types
template <typename T> using Ptr = std::unique_ptr<T, GObjectDeleter>;

// Take ownership of a floating reference (typical for newly created widgets)
template <typename T> [[nodiscard]] auto take_floating(T *obj) -> Ptr<T> {
  if (obj) {
    g_object_ref_sink(obj);
  }
  return Ptr<T>(obj);
}

// Take ownership by adding a reference
template <typename T> [[nodiscard]] auto take_ref(T *obj) -> Ptr<T> {
  if (obj) {
    g_object_ref(obj);
  }
  return Ptr<T>(obj);
}

// RAII wrapper for signal connections
class SignalConnection {
public:
  SignalConnection() noexcept = default;

  SignalConnection(GObject *instance, gulong handler_id) noexcept
      : instance_(instance), handler_id_(handler_id) {
    if (instance_) {
      g_object_ref(instance_);
    }
  }

  ~SignalConnection() { disconnect(); }

  // Move-only
  SignalConnection(SignalConnection &&other) noexcept
      : instance_(std::exchange(other.instance_, nullptr)),
        handler_id_(std::exchange(other.handler_id_, 0)) {}

  auto operator=(SignalConnection &&other) noexcept -> SignalConnection & {
    if (this != &other) {
      disconnect();
      instance_ = std::exchange(other.instance_, nullptr);
      handler_id_ = std::exchange(other.handler_id_, 0);
    }
    return *this;
  }

  SignalConnection(SignalConnection const &) = delete;
  auto operator=(SignalConnection const &) -> SignalConnection & = delete;

  void disconnect() noexcept {
    if (instance_ && handler_id_ != 0) {
      g_signal_handler_disconnect(instance_, handler_id_);
      g_object_unref(instance_);
      instance_ = nullptr;
      handler_id_ = 0;
    }
  }

  [[nodiscard]] auto is_connected() const noexcept -> bool {
    return instance_ != nullptr && handler_id_ != 0;
  }

private:
  GObject *instance_ = nullptr;
  gulong handler_id_ = 0;
};

// Type-safe signal connection with stored callback
template <typename Callback> class Signal {
public:
  Signal() = default;

  template <typename F>
  Signal(GObject *instance, char const *signal_name, F &&callback)
      : callback_(std::make_unique<Callback>(std::forward<F>(callback))) {
    auto const handler_id = g_signal_connect_data(
        instance, signal_name, G_CALLBACK(&Signal::invoke), callback_.get(),
        nullptr, G_CONNECT_DEFAULT);
    connection_ = SignalConnection(instance, handler_id);
  }

  ~Signal() = default;

  Signal(Signal &&) noexcept = default;
  auto operator=(Signal &&) noexcept -> Signal & = default;

  Signal(Signal const &) = delete;
  auto operator=(Signal const &) -> Signal & = delete;

  void disconnect() { connection_.disconnect(); }

private:
  static void invoke(gpointer, gpointer user_data) {
    auto *cb = static_cast<Callback *>(user_data);
    (*cb)();
  }

  std::unique_ptr<Callback> callback_;
  SignalConnection connection_;
};

// Helper to connect signals with lambdas
template <typename F>
[[nodiscard]] auto connect(GObject *instance, char const *signal_name,
                           F &&callback) -> Signal<std::decay_t<F>> {
  return Signal<std::decay_t<F>>(instance, signal_name,
                                 std::forward<F>(callback));
}

} // namespace gtk
