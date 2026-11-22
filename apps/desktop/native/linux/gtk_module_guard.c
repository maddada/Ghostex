/*
CDXC:PlatformSupport 2026-11-22 WHY:
Chromium 156, the CEF build this app ships, calls
g_object_set(settings, "gtk-modules", "", NULL) after GTK has already loaded
the modules named in settings.ini. KDE Plasma puts
colorreload-gtk-module and window-decorations-gtk-module there (and
appmenu-gtk-module when the global menu is on). GTK then g_module_close()s
them while their theme callbacks are still registered, so the next desktop
theme change jumps to unmapped code and the runtime dies with SIGSEGV
(SEGV_ACCERR). Chromium's GtkSettings interceptor forces the same empty
value through set_property, which unloads the modules the same way.

This executable exports g_object_set and g_module_close. The empty pin is
ignored. If an unload still reaches g_module_close, those modules stay
mapped and their callbacks keep running real code. Every other call is
forwarded. libcef.so is DT_NEEDED, so the symbols have to be in the
executable; LD_PRELOAD set from main is too late.
*/

#define _GNU_SOURCE

#include <dlfcn.h>
#include <stdarg.h>
#include <string.h>

typedef void (*g_object_set_valist_fn)(void *, const char *, va_list);
typedef int (*g_module_close_fn)(void *);
typedef const char *(*g_module_name_fn)(void *);

static void *object_set_valist_slot = NULL;
static void *real_module_close_slot = NULL;
static void *module_name_slot = NULL;

static void *resolve_symbol(void **slot, void *handle, const char *symbol) {
    void *fn = __atomic_load_n(slot, __ATOMIC_ACQUIRE);
    if (fn != NULL) {
        return fn;
    }
    fn = dlsym(handle, symbol);
    if (fn != NULL) {
        __atomic_store_n(slot, fn, __ATOMIC_RELEASE);
    }
    return fn;
}

__attribute__((visibility("default"))) void g_object_set(void *object, const char *first_property, ...) {
    if (first_property != NULL && strcmp(first_property, "gtk-modules") == 0) {
        va_list peek;
        va_start(peek, first_property);
        const char *value = va_arg(peek, const char *);
        const char *next = va_arg(peek, const char *);
        va_end(peek);
        if (value != NULL && value[0] == '\0' && next == NULL) {
            return;
        }
    }

    g_object_set_valist_fn object_set_valist =
        (g_object_set_valist_fn)resolve_symbol(&object_set_valist_slot, RTLD_NEXT, "g_object_set_valist");
    if (object_set_valist == NULL) {
        return;
    }
    va_list args;
    va_start(args, first_property);
    object_set_valist(object, first_property, args);
    va_end(args);
}

static int is_kept_gtk_module(void *module) {
    if (module == NULL) {
        return 0;
    }
    g_module_name_fn module_name =
        (g_module_name_fn)resolve_symbol(&module_name_slot, RTLD_DEFAULT, "g_module_name");
    if (module_name == NULL) {
        return 0;
    }
    const char *name = module_name(module);
    if (name == NULL) {
        return 0;
    }
    return strstr(name, "colorreload-gtk-module") != NULL ||
           strstr(name, "window-decorations-gtk-module") != NULL ||
           strstr(name, "appmenu-gtk-module") != NULL;
}

__attribute__((visibility("default"))) int g_module_close(void *module) {
    if (is_kept_gtk_module(module)) {
        return 1;
    }
    g_module_close_fn real_module_close =
        (g_module_close_fn)resolve_symbol(&real_module_close_slot, RTLD_NEXT, "g_module_close");
    if (real_module_close == NULL) {
        return 0;
    }
    return real_module_close(module);
}
