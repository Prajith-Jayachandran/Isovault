#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/types.h>

#define GUI_BIN "/home/prajithj/Isovault/gui/isovault_gui"

int main(int argc, char *argv[]) {
    // If already root, execute GUI directly
    if (geteuid() == 0) {
        char *gui_args[] = { GUI_BIN, NULL };
        execv(GUI_BIN, gui_args);
        perror("Failed to execute GUI binary");
        return 1;
    }

    // Auto-authorize local root to connect to user's X11/XWayland display
    system("xhost +si:localuser:root >/dev/null 2>&1");

    const char *display = getenv("DISPLAY");
    if (!display || strlen(display) == 0) {
        display = ":0";
    }

    // Check for pkexec (Polkit graphical elevation)
    if (system("which pkexec >/dev/null 2>&1") == 0) {
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "pkexec env DISPLAY=%s GDK_BACKEND=x11 %s", display, GUI_BIN);
        return system(cmd);
    }

    // Fallback to sudo
    if (system("which sudo >/dev/null 2>&1") == 0) {
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "sudo -E env DISPLAY=%s GDK_BACKEND=x11 %s", display, GUI_BIN);
        return system(cmd);
    }

    fprintf(stderr, "Error: Root privileges required. Neither pkexec nor sudo was found.\n");
    return 1;
}
