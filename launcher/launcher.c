#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/types.h>

#define GUI_BIN "/home/prajithj/isovault/gui/isovault_gui"

int main(int argc, char *argv[]) {
    // If already root, execute GUI directly
    if (geteuid() == 0) {
        char *gui_args[] = { GUI_BIN, NULL };
        execv(GUI_BIN, gui_args);
        perror("Failed to execute GUI binary");
        return 1;
    }

    // Preserve DISPLAY and WAYLAND_DISPLAY if available
    const char *display = getenv("DISPLAY");
    const char *wayland = getenv("WAYLAND_DISPLAY");

    // Check for pkexec (Polkit graphical elevation)
    if (system("which pkexec >/dev/null 2>&1") == 0) {
        char cmd[1024];
        if (display && wayland) {
            snprintf(cmd, sizeof(cmd), "pkexec env DISPLAY=%s WAYLAND_DISPLAY=%s %s", display, wayland, GUI_BIN);
        } else if (display) {
            snprintf(cmd, sizeof(cmd), "pkexec env DISPLAY=%s %s", display, GUI_BIN);
        } else {
            snprintf(cmd, sizeof(cmd), "pkexec %s", GUI_BIN);
        }
        return system(cmd);
    }

    // Fallback to sudo
    if (system("which sudo >/dev/null 2>&1") == 0) {
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "sudo -E %s", GUI_BIN);
        return system(cmd);
    }

    fprintf(stderr, "Error: Root privileges required. Neither pkexec nor sudo was found.\n");
    return 1;
}
