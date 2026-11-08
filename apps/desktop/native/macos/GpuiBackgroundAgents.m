#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#include <stdint.h>
#include <string.h>

/*
 CDXC:BackgroundItems 2026-11-08 WHY: GitHub #510. User: "do it now for 11.1.2", so
 Ghostex's background helpers are registered with SMAppService from plists the app
 ships in Contents/Library/LaunchAgents. macOS then lists them under Ghostex in
 Login Items and asks once per install instead of after every update (legacy plists
 in ~/Library/LaunchAgents are re-checked, and announced again, whenever their
 program changes). These calls are a thin C surface; every decision, including the
 fallback to the legacy plists, is in board_gxserver/background_agents.rs.

 Status values, matching SMAppServiceStatus, plus -1 when this macOS has no
 SMAppService (older than 13), which Rust treats like a refused registration.
*/

static const int32_t GhostexBackgroundAgentUnavailable = -1;

static void GhostexBackgroundAgentCopyError(NSError *error, char *buffer,
                                            size_t length) {
  if (buffer == NULL || length == 0) {
    return;
  }
  buffer[0] = 0;
  if (error == nil) {
    return;
  }
  NSString *text = [NSString
      stringWithFormat:@"%@ (%@ %ld)", error.localizedDescription,
                       error.domain, (long)error.code];
  strlcpy(buffer, text.UTF8String ?: "", length);
}

static NSString *GhostexBackgroundAgentPlistName(const char *plist_name) {
  if (plist_name == NULL) {
    return nil;
  }
  NSString *name = [NSString stringWithUTF8String:plist_name];
  return name.length > 0 ? name : nil;
}

int32_t ghostex_gpui_background_agent_status(const char *plist_name) {
  if (@available(macOS 13.0, *)) {
    NSString *name = GhostexBackgroundAgentPlistName(plist_name);
    if (name == nil) {
      return GhostexBackgroundAgentUnavailable;
    }
    return (int32_t)[SMAppService agentServiceWithPlistName:name].status;
  }
  return GhostexBackgroundAgentUnavailable;
}

/// Registers the agent (a no-op when it is already enabled) and returns its status
/// afterwards. A failed call leaves its reason in `error_buffer`.
int32_t ghostex_gpui_background_agent_register(const char *plist_name,
                                               char *error_buffer,
                                               size_t error_length) {
  if (@available(macOS 13.0, *)) {
    NSString *name = GhostexBackgroundAgentPlistName(plist_name);
    if (name == nil) {
      return GhostexBackgroundAgentUnavailable;
    }
    SMAppService *service = [SMAppService agentServiceWithPlistName:name];
    if (service.status != SMAppServiceStatusEnabled) {
      NSError *error = nil;
      if (![service registerAndReturnError:&error]) {
        GhostexBackgroundAgentCopyError(error, error_buffer, error_length);
      }
    }
    return (int32_t)service.status;
  }
  return GhostexBackgroundAgentUnavailable;
}

/// Unregisters the agent; launchd unloads it. Returns 1 on success or when it was
/// not registered.
int32_t ghostex_gpui_background_agent_unregister(const char *plist_name,
                                                 char *error_buffer,
                                                 size_t error_length) {
  if (@available(macOS 13.0, *)) {
    NSString *name = GhostexBackgroundAgentPlistName(plist_name);
    if (name == nil) {
      return 0;
    }
    SMAppService *service = [SMAppService agentServiceWithPlistName:name];
    if (service.status == SMAppServiceStatusNotRegistered ||
        service.status == SMAppServiceStatusNotFound) {
      return 1;
    }
    NSError *error = nil;
    if ([service unregisterAndReturnError:&error]) {
      return 1;
    }
    GhostexBackgroundAgentCopyError(error, error_buffer, error_length);
    return 0;
  }
  return 0;
}

/// Opens System Settings > General > Login Items, where the user approves a helper
/// that needs it.
void ghostex_gpui_background_agent_open_login_items(void) {
  if (@available(macOS 13.0, *)) {
    [SMAppService openSystemSettingsLoginItems];
  }
}
