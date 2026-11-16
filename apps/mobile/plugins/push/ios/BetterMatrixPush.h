// The engine's C entry point for pushes (packages/matrix-ffi/src/push.rs), for the Notification
// Service Extension. The symbols live in the engine's static library (libmatrix_ffi.a in
// packages/matrix-native/build/MatrixNativeFramework.xcframework), which the extension links.

#pragma once
#include <stdbool.h>

/// Reads the keychain item `key`: sets `*value` to a string allocated with malloc (the engine
/// frees it), or leaves it NULL when there is none. False when the keychain failed.
typedef bool (*bm_secret_get_fn)(void *context, const char *key, char **value);
/// Writes the keychain item `key`.
typedef bool (*bm_secret_set_fn)(void *context, const char *key, const char *value);
/// Removes the keychain item `key`; a missing one is removed already.
typedef bool (*bm_secret_delete_fn)(void *context, const char *key);

/// The extension's keychain, in the field order of `matrix_ffi::push::BmSecrets`.
typedef struct {
  void *context;
  bm_secret_get_fn get;
  bm_secret_set_fn set;
  bm_secret_delete_fn delete;
} BmSecrets;

/// Handles one push: `payload` is the notification's userInfo as JSON, `data_dir` the engine's
/// data directory in the App Group container. Blocks for up to about 25 seconds. Returns JSON
/// (`{"target": …, "app_active": bool, "shown": {"title", "body", "thread"} | null}`) to free
/// with bm_push_free, or NULL.
char *bm_push_fetch(const char *payload, const char *data_dir, BmSecrets secrets);

/// Frees what bm_push_fetch returned.
void bm_push_free(char *json);
