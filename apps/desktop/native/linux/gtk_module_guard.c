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
#include <pthread.h>
#include <stdarg.h>
#include <string.h>

static int is_empty_gtk_modules_pin(const char *name, const char *value) {
    return name != NULL && value != NULL && strcmp(name, "gtk-modules") == 0 && value[0] == '\0';
}

static void (*object_set_valist)(void *, const char *, va_list) = NULL;

static void resolve_object_set(void) {
    object_set_valist = (void (*)(void *, const char *, va_list))dlsym(RTLD_NEXT, "g_object_set_valist");
}

__attribute__((visibility("default"))) void g_object_set(void *object, const char *first_property, ...) {
    static pthread_once_t once = PTHREAD_ONCE_INIT;
    pthread_once(&once, resolve_object_set);

    if (first_property != NULL) {
        va_list peek;
        va_start(peek, first_property);
        const char *value = va_arg(peek, const char *);
        const char *next = va_arg(peek, const char *);
        va_end(peek);
        if (is_empty_gtk_modules_pin(first_property, value) && next == NULL) {
            return;
        }
    }

    if (object_set_valist == NULL) {
        return;
    }
    va_list args;
    va_start(args, first_property);
    object_set_valist(object, first_property, args);
    va_end(args);
}

static int (*real_module_close)(void *) = NULL;
static const char *(*module_name)(void *) = NULL;

static void resolve_module_close(void) {
    real_module_close = (int (*)(void *))dlsym(RTLD_NEXT, "g_module_close");
    module_name = (const char *(*)(void *))dlsym(RTLD_DEFAULT, "g_module_name");
}

static int is_kept_gtk_module(void *module) {
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
    static pthread_once_t once = PTHREAD_ONCE_INIT;
    pthread_once(&once, resolve_module_close);
    if (is_kept_gtk_module(module)) {
        return 1;
    }
    if (real_module_close == NULL) {
        return 0;
    }
    return real_module_close(module);
}
