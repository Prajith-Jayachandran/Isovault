#include <gtk/gtk.h>
#include <json-glib/json-glib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PACKAGE_VERSION "1.1.18"
#define ENGINE_PATH "/home/prajithj/isovault/engine/target/release/isovault_engine"
#define GLADE_PATH "/home/prajithj/isovault/gui/isovault_gui.glade"

typedef struct {
    char name[64];
    char path[128];
    char size_formatted[32];
    char model[128];
    char vendor[64];
    char installed_version[32];
    char installed_style[16];
    gboolean is_installed;
    gboolean is_removable;
} DeviceInfo;

typedef struct {
    GtkWidget *main_window;
    GtkWidget *dev_combo;
    GtkWidget *refresh_btn;
    GtkWidget *device_ver_label;
    GtkWidget *package_ver_label;
    GtkWidget *radio_mbr;
    GtkWidget *radio_gpt;
    GtkWidget *fs_combo;
    GtkWidget *show_all_check;
    GtkWidget *install_btn;
    GtkWidget *update_btn;
    GtkWidget *progress_bar;
    GtkWidget *status_label;

    GList *device_list;
    GPid worker_pid;
    gint worker_stdout_fd;
    guint worker_watch_id;
    gboolean is_working;
} AppWidgets;

static void refresh_devices(AppWidgets *app);
static void update_device_info_display(AppWidgets *app);

static void free_device_list(AppWidgets *app) {
    GList *iter;
    for (iter = app->device_list; iter != NULL; iter = iter->next) {
        g_free(iter->data);
    }
    g_list_free(app->device_list);
    app->device_list = NULL;
}

static DeviceInfo *get_selected_device(AppWidgets *app) {
    gint active_idx = gtk_combo_box_get_active(GTK_COMBO_BOX(app->dev_combo));
    if (active_idx < 0) return NULL;
    return (DeviceInfo *)g_list_nth_data(app->device_list, active_idx);
}

static void on_dev_combo_changed(GtkComboBox *combo, gpointer user_data) {
    AppWidgets *app = (AppWidgets *)user_data;
    update_device_info_display(app);
}

static void update_device_info_display(AppWidgets *app) {
    DeviceInfo *dev = get_selected_device(app);
    if (!dev) {
        gtk_label_set_text(GTK_LABEL(app->device_ver_label), "None");
        gtk_widget_set_sensitive(app->install_btn, FALSE);
        gtk_widget_set_sensitive(app->update_btn, FALSE);
        return;
    }

    if (dev->is_installed) {
        char buf[64];
        snprintf(buf, sizeof(buf), "v%s (%s)", dev->installed_version, dev->installed_style);
        gtk_label_set_text(GTK_LABEL(app->device_ver_label), buf);
        gtk_widget_set_sensitive(app->update_btn, !app->is_working);
    } else {
        gtk_label_set_text(GTK_LABEL(app->device_ver_label), "Not Installed");
        gtk_widget_set_sensitive(app->update_btn, FALSE);
    }

    gtk_widget_set_sensitive(app->install_btn, !app->is_working);
}

static void parse_devices_json(AppWidgets *app, const gchar *json_str) {
    free_device_list(app);
    gtk_combo_box_text_remove_all(GTK_COMBO_BOX_TEXT(app->dev_combo));

    JsonParser *parser = json_parser_new();
    GError *error = NULL;

    if (!json_parser_load_from_data(parser, json_str, -1, &error)) {
        g_warning("Failed to parse device JSON: %s", error ? error->message : "unknown");
        if (error) g_error_free(error);
        g_object_unref(parser);
        return;
    }

    JsonNode *root = json_parser_get_root(parser);
    if (!root || json_node_get_node_type(root) != JSON_NODE_ARRAY) {
        g_object_unref(parser);
        return;
    }

    JsonArray *arr = json_node_get_array(root);
    guint len = json_array_get_length(arr);

    for (guint i = 0; i < len; i++) {
        JsonObject *obj = json_array_get_object_element(arr, i);
        DeviceInfo *dev = g_new0(DeviceInfo, 1);

        const gchar *path = json_object_get_string_member_with_default(obj, "path", "");
        const gchar *size = json_object_get_string_member_with_default(obj, "size_formatted", "");
        const gchar *model = json_object_get_string_member_with_default(obj, "model", "");
        const gchar *vendor = json_object_get_string_member_with_default(obj, "vendor", "");
        const gchar *inst_ver = json_object_get_string_member_with_default(obj, "installed_version", "None");
        const gchar *inst_style = json_object_get_string_member_with_default(obj, "installed_style", "None");
        gboolean is_inst = json_object_get_boolean_member_with_default(obj, "is_installed", FALSE);
        gboolean is_rem = json_object_get_boolean_member_with_default(obj, "is_removable", FALSE);

        g_strlcpy(dev->path, path, sizeof(dev->path));
        g_strlcpy(dev->size_formatted, size, sizeof(dev->size_formatted));
        g_strlcpy(dev->model, model, sizeof(dev->model));
        g_strlcpy(dev->vendor, vendor, sizeof(dev->vendor));
        g_strlcpy(dev->installed_version, inst_ver, sizeof(dev->installed_version));
        g_strlcpy(dev->installed_style, inst_style, sizeof(dev->installed_style));
        dev->is_installed = is_inst;
        dev->is_removable = is_rem;

        char item_text[512];
        snprintf(item_text, sizeof(item_text), "%s  [%s]  %s %s", dev->path, dev->size_formatted, dev->vendor, dev->model);
        gtk_combo_box_text_append_text(GTK_COMBO_BOX_TEXT(app->dev_combo), item_text);

        app->device_list = g_list_append(app->device_list, dev);
    }

    g_object_unref(parser);

    if (len > 0) {
        gtk_combo_box_set_active(GTK_COMBO_BOX(app->dev_combo), 0);
    }

    update_device_info_display(app);
}

static void refresh_devices(AppWidgets *app) {
    gboolean show_all = gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(app->show_all_check));
    gchar *stdout_str = NULL;
    gchar *stderr_str = NULL;
    gint exit_status = 0;
    GError *error = NULL;

    const gchar *argv[] = {ENGINE_PATH, "list", "--json", NULL, NULL};
    if (show_all) {
        argv[3] = "--all";
    }

    if (g_spawn_sync(NULL, (gchar **)argv, NULL, G_SPAWN_SEARCH_PATH, NULL, NULL, &stdout_str, &stderr_str, &exit_status, &error)) {
        if (exit_status == 0 && stdout_str) {
            parse_devices_json(app, stdout_str);
        }
        g_free(stdout_str);
        g_free(stderr_str);
    } else {
        g_warning("Failed to run engine list: %s", error ? error->message : "unknown");
        if (error) g_error_free(error);
    }
}

static void on_refresh_clicked(GtkButton *btn, gpointer user_data) {
    AppWidgets *app = (AppWidgets *)user_data;
    refresh_devices(app);
}

static void on_show_all_toggled(GtkToggleButton *btn, gpointer user_data) {
    AppWidgets *app = (AppWidgets *)user_data;
    refresh_devices(app);
}

static void set_ui_busy(AppWidgets *app, gboolean busy) {
    app->is_working = busy;
    gtk_widget_set_sensitive(app->dev_combo, !busy);
    gtk_widget_set_sensitive(app->refresh_btn, !busy);
    gtk_widget_set_sensitive(app->radio_mbr, !busy);
    gtk_widget_set_sensitive(app->radio_gpt, !busy);
    gtk_widget_set_sensitive(app->fs_combo, !busy);
    gtk_widget_set_sensitive(app->show_all_check, !busy);
    gtk_widget_set_sensitive(app->install_btn, !busy);
    gtk_widget_set_sensitive(app->update_btn, !busy);

    if (busy) {
        gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(app->progress_bar), 0.0);
        gtk_progress_bar_set_text(GTK_PROGRESS_BAR(app->progress_bar), "Starting...");
    }
}

static gboolean on_worker_stdout(GIOChannel *channel, GIOCondition cond, gpointer user_data) {
    AppWidgets *app = (AppWidgets *)user_data;

    if (cond & (G_IO_IN | G_IO_PRI)) {
        gchar *line = NULL;
        gsize len = 0;
        GError *error = NULL;

        while (g_io_channel_read_line(channel, &line, &len, NULL, &error) == G_IO_STATUS_NORMAL) {
            if (line && len > 0) {
                g_strstrip(line);
                if (line[0] == '{') {
                    JsonParser *parser = json_parser_new();
                    if (json_parser_load_from_data(parser, line, -1, NULL)) {
                        JsonNode *root = json_parser_get_root(parser);
                        if (root && json_node_get_node_type(root) == JSON_NODE_OBJECT) {
                            JsonObject *obj = json_node_get_object(root);
                            gint percent = json_object_get_int_member_with_default(obj, "percent", 0);
                            const gchar *msg = json_object_get_string_member_with_default(obj, "message", "");
                            const gchar *status = json_object_get_string_member_with_default(obj, "status", "");

                            gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(app->progress_bar), (gdouble)percent / 100.0);
                            char pbuf[32];
                            snprintf(pbuf, sizeof(pbuf), "%d%%", percent);
                            gtk_progress_bar_set_text(GTK_PROGRESS_BAR(app->progress_bar), pbuf);
                            gtk_label_set_text(GTK_LABEL(app->status_label), msg);

                            if (g_strcmp0(status, "success") == 0) {
                                GtkWidget *dlg = gtk_message_dialog_new(
                                    GTK_WINDOW(app->main_window),
                                    GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT,
                                    GTK_MESSAGE_INFO,
                                    GTK_BUTTONS_OK,
                                    "Operation successfully completed!\n\n%s",
                                    msg
                                );
                                gtk_dialog_run(GTK_DIALOG(dlg));
                                gtk_widget_destroy(dlg);
                            } else if (g_strcmp0(status, "error") == 0) {
                                GtkWidget *dlg = gtk_message_dialog_new(
                                    GTK_WINDOW(app->main_window),
                                    GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT,
                                    GTK_MESSAGE_ERROR,
                                    GTK_BUTTONS_OK,
                                    "Operation failed:\n\n%s",
                                    msg
                                );
                                gtk_dialog_run(GTK_DIALOG(dlg));
                                gtk_widget_destroy(dlg);
                            }
                        }
                    }
                    g_object_unref(parser);
                } else {
                    gtk_label_set_text(GTK_LABEL(app->status_label), line);
                }
                g_free(line);
                line = NULL;
            }
        }
        if (error) g_error_free(error);
    }

    if (cond & (G_IO_HUP | G_IO_ERR)) {
        set_ui_busy(app, FALSE);
        refresh_devices(app);
        return FALSE; // Remove source
    }

    return TRUE;
}

static void run_engine_command(AppWidgets *app, const gchar **argv) {
    GError *error = NULL;
    set_ui_busy(app, TRUE);

    if (!g_spawn_async_with_pipes(
            NULL,
            (gchar **)argv,
            NULL,
            G_SPAWN_DO_NOT_REAP_CHILD | G_SPAWN_SEARCH_PATH,
            NULL,
            NULL,
            &app->worker_pid,
            NULL,
            &app->worker_stdout_fd,
            NULL,
            &error)) {
        set_ui_busy(app, FALSE);
        GtkWidget *dlg = gtk_message_dialog_new(
            GTK_WINDOW(app->main_window),
            GTK_DIALOG_MODAL,
            GTK_MESSAGE_ERROR,
            GTK_BUTTONS_OK,
            "Failed to launch IsoVault engine: %s",
            error ? error->message : "unknown"
        );
        gtk_dialog_run(GTK_DIALOG(dlg));
        gtk_widget_destroy(dlg);
        if (error) g_error_free(error);
        return;
    }

    GIOChannel *channel = g_io_channel_unix_new(app->worker_stdout_fd);
    g_io_channel_set_encoding(channel, NULL, NULL);
    g_io_channel_set_flags(channel, G_IO_FLAG_NONBLOCK, NULL);
    app->worker_watch_id = g_io_add_watch(channel, G_IO_IN | G_IO_HUP | G_IO_ERR | G_IO_PRI, on_worker_stdout, app);
    g_io_channel_unref(channel);
}

static void on_install_clicked(GtkButton *btn, gpointer user_data) {
    AppWidgets *app = (AppWidgets *)user_data;
    DeviceInfo *dev = get_selected_device(app);
    if (!dev) return;

    // First confirmation dialog
    GtkWidget *dlg1 = gtk_message_dialog_new(
        GTK_WINDOW(app->main_window),
        GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT,
        GTK_MESSAGE_WARNING,
        GTK_BUTTONS_YES_NO,
        "ATTENTION:\n\nYou will install IsoVault to %s (%s, %s).\n\nAll data on this disk will be PERMANENTLY ERASED!\n\nAre you sure you want to continue?",
        dev->path, dev->size_formatted, dev->model
    );
    gtk_window_set_title(GTK_WINDOW(dlg1), "Warning: Data Loss");
    gint resp1 = gtk_dialog_run(GTK_DIALOG(dlg1));
    gtk_widget_destroy(dlg1);

    if (resp1 != GTK_RESPONSE_YES) return;

    // Second double-check dialog
    GtkWidget *dlg2 = gtk_message_dialog_new(
        GTK_WINDOW(app->main_window),
        GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT,
        GTK_MESSAGE_QUESTION,
        GTK_BUTTONS_YES_NO,
        "DOUBLE-CHECK:\n\nAll files on %s will be lost and cannot be recovered.\n\nProceed with formatting?",
        dev->path
    );
    gtk_window_set_title(GTK_WINDOW(dlg2), "Confirm Format");
    gint resp2 = gtk_dialog_run(GTK_DIALOG(dlg2));
    gtk_widget_destroy(dlg2);

    if (resp2 != GTK_RESPONSE_YES) return;

    const gchar *style = gtk_toggle_button_get_active(GTK_TOGGLE_BUTTON(app->radio_gpt)) ? "gpt" : "mbr";
    const gchar *fs = (gtk_combo_box_get_active(GTK_COMBO_BOX(app->fs_combo)) == 1) ? "fat32" : "exfat";

    const gchar *argv[] = {
        ENGINE_PATH,
        "install",
        "--device", dev->path,
        "--style", style,
        "--fs", fs,
        "--json",
        NULL
    };

    run_engine_command(app, argv);
}

static void on_update_clicked(GtkButton *btn, gpointer user_data) {
    AppWidgets *app = (AppWidgets *)user_data;
    DeviceInfo *dev = get_selected_device(app);
    if (!dev || !dev->is_installed) return;

    GtkWidget *dlg = gtk_message_dialog_new(
        GTK_WINDOW(app->main_window),
        GTK_DIALOG_MODAL | GTK_DIALOG_DESTROY_WITH_PARENT,
        GTK_MESSAGE_QUESTION,
        GTK_BUTTONS_YES_NO,
        "Update IsoVault bootloader on %s to version %s?\n\nYour ISO files and data on partition 1 will be preserved untouched.",
        dev->path, PACKAGE_VERSION
    );
    gtk_window_set_title(GTK_WINDOW(dlg), "Update Bootloader");
    gint resp = gtk_dialog_run(GTK_DIALOG(dlg));
    gtk_widget_destroy(dlg);

    if (resp != GTK_RESPONSE_YES) return;

    const gchar *argv[] = {
        ENGINE_PATH,
        "update",
        "--device", dev->path,
        "--json",
        NULL
    };

    run_engine_command(app, argv);
}

int main(int argc, char *argv[]) {
    gtk_init(&argc, &argv);

    GtkBuilder *builder = gtk_builder_new();
    GError *error = NULL;

    if (!gtk_builder_add_from_file(builder, GLADE_PATH, &error)) {
        g_printerr("Failed to load Glade file: %s\n", error ? error->message : "unknown");
        if (error) g_error_free(error);
        return 1;
    }

    AppWidgets *app = g_new0(AppWidgets, 1);

    app->main_window = GTK_WIDGET(gtk_builder_get_object(builder, "main_window"));
    app->dev_combo = GTK_WIDGET(gtk_builder_get_object(builder, "dev_combo"));
    app->refresh_btn = GTK_WIDGET(gtk_builder_get_object(builder, "refresh_btn"));
    app->device_ver_label = GTK_WIDGET(gtk_builder_get_object(builder, "device_ver_label"));
    app->package_ver_label = GTK_WIDGET(gtk_builder_get_object(builder, "package_ver_label"));
    app->radio_mbr = GTK_WIDGET(gtk_builder_get_object(builder, "radio_mbr"));
    app->radio_gpt = GTK_WIDGET(gtk_builder_get_object(builder, "radio_gpt"));
    app->fs_combo = GTK_WIDGET(gtk_builder_get_object(builder, "fs_combo"));
    app->show_all_check = GTK_WIDGET(gtk_builder_get_object(builder, "show_all_check"));
    app->install_btn = GTK_WIDGET(gtk_builder_get_object(builder, "install_btn"));
    app->update_btn = GTK_WIDGET(gtk_builder_get_object(builder, "update_btn"));
    app->progress_bar = GTK_WIDGET(gtk_builder_get_object(builder, "progress_bar"));
    app->status_label = GTK_WIDGET(gtk_builder_get_object(builder, "status_label"));

    gtk_label_set_text(GTK_LABEL(app->package_ver_label), PACKAGE_VERSION);
    gtk_combo_box_set_active(GTK_COMBO_BOX(app->fs_combo), 0); // exFAT default

    g_signal_connect(app->main_window, "destroy", G_CALLBACK(gtk_main_quit), NULL);
    g_signal_connect(app->refresh_btn, "clicked", G_CALLBACK(on_refresh_clicked), app);
    g_signal_connect(app->dev_combo, "changed", G_CALLBACK(on_dev_combo_changed), app);
    g_signal_connect(app->show_all_check, "toggled", G_CALLBACK(on_show_all_toggled), app);
    g_signal_connect(app->install_btn, "clicked", G_CALLBACK(on_install_clicked), app);
    g_signal_connect(app->update_btn, "clicked", G_CALLBACK(on_update_clicked), app);

    refresh_devices(app);

    gtk_widget_show_all(app->main_window);
    gtk_main();

    free_device_list(app);
    g_free(app);
    g_object_unref(builder);

    return 0;
}
