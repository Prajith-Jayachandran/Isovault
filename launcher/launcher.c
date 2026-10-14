#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/types.h>
#include <libgen.h>
#include <limits.h>

static void resolve_gui_bin(char *out_path, size_t max_len) {
    char exe_path[PATH_MAX] = {0};
    ssize_t len = readlink("/proc/self/exe", exe_path, sizeof(exe_path) - 1);
    if (len > 0) {
        exe_path[len] = '\0';
        char *dir = dirname(exe_path);
        // If launcher is at <project>/IsoVaultGUI, GUI binary is at <project>/gui/isovault_gui
        snprintf(out_path, max_len, "%s/gui/isovault_gui", dir);
        if (access(out_path, X_OK) == 0) return;

        // If launcher is in <project>/launcher/, GUI binary is at <project>/gui/isovault_gui
        char parent[PATH_MAX];
        strncpy(parent, dir, sizeof(parent) - 1);
        snprintf(out_path, max_len, "%s/../gui/isovault_gui", parent);
        if (access(out_path, X_OK) == 0) return;
    }

    // Check current working directory
    if (access("./gui/isovault_gui", X_OK) == 0) {
        strncpy(out_path, "./gui/isovault_gui", max_len - 1);
        return;
    }

    // Check known home directory paths
    const char *home = getenv("HOME");
    if (home) {
        snprintf(out_path, max_len, "%s/Isovault/gui/isovault_gui", home);
        if (access(out_path, X_OK) == 0) return;

        snprintf(out_path, max_len, "%s/isovault/gui/isovault_gui", home);
        if (access(out_path, X_OK) == 0) return;
    }

    // Fallback candidate
    if (exe_path[0]) {
        snprintf(out_path, max_len, "%s/gui/isovault_gui", dirname(exe_path));
    } else {
        strncpy(out_path, "./gui/isovault_gui", max_len - 1);
    }
}

int main(int argc, char *argv[]) {
    char gui_bin[PATH_MAX * 2] = {0};
    resolve_gui_bin(gui_bin, sizeof(gui_bin));

    // If already root, execute GUI directly
    if (geteuid() == 0) {
        char *gui_args[] = { gui_bin, NULL };
        execv(gui_bin, gui_args);
        perror("Failed to execute GUI binary");
        return 1;
    }

    // Auto-authorize local root to connect to user's X11/XWayland display
    system("xhost +si:localuser:root >/dev/null 2>&1");

    const char *display = getenv("DISPLAY");
    if (!display || strlen(display) == 0) {
        display = ":1";
    }

    // Check for pkexec (Polkit graphical elevation)
    if (system("which pkexec >/dev/null 2>&1") == 0) {
        char cmd[2048];
        snprintf(cmd, sizeof(cmd), "pkexec env DISPLAY=%s GDK_BACKEND=x11 %s", display, gui_bin);
        return system(cmd);
    }

    // Fallback to sudo
    if (system("which sudo >/dev/null 2>&1") == 0) {
        char cmd[2048];
        snprintf(cmd, sizeof(cmd), "sudo -E env DISPLAY=%s GDK_BACKEND=x11 %s", display, gui_bin);
        return system(cmd);
    }

    fprintf(stderr, "Error: Root privileges required. Neither pkexec nor sudo was found.\n");
    return 1;
}
