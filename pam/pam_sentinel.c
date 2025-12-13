/*
 * pam_sentinel.c - PAM module for graphical confirmation dialogs
 *
 * Copyright (C) 2024
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

/* Must be defined before any includes for POSIX functions */
#define _GNU_SOURCE

#define PAM_SM_AUTH

#include <stdio.h>
#include <signal.h>
#include <sys/select.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <pwd.h>
#include <syslog.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <fcntl.h>
#include <dirent.h>
#include <grp.h>

#include <security/pam_modules.h>
#include <security/pam_ext.h>

#include "config.h"

#define MODULE_NAME "pam_sentinel"
#define MAX_LINE 1024
#define SOCKET_TIMEOUT 60

/* Configuration structure */
typedef struct {
    int enabled;
    int timeout;
    int randomize_buttons;
    int show_process_info;
    int log_attempts;
    int min_display_time;
    char headless_action[32];
    char title[256];
    char message[512];
    char secondary[512];
} confirm_config_t;

/* Try to find and set WAYLAND_DISPLAY if not already set */
static int find_wayland_display(uid_t uid)
{
    char runtime_dir[256];
    DIR *dir;
    struct dirent *entry;

    /* First check if already set */
    const char *wayland = getenv("WAYLAND_DISPLAY");
    if (wayland && *wayland) {
        return 1;
    }

    /* Try to find wayland socket in user's runtime dir */
    snprintf(runtime_dir, sizeof(runtime_dir), "/run/user/%d", uid);

    dir = opendir(runtime_dir);
    if (!dir) {
        return 0;
    }

    while ((entry = readdir(dir)) != NULL) {
        /* Look for wayland-* socket */
        if (strncmp(entry->d_name, "wayland-", 8) == 0 &&
            strstr(entry->d_name, ".lock") == NULL) {
            /* Found a wayland socket, set the env var */
            setenv("WAYLAND_DISPLAY", entry->d_name, 1);

            /* Also set XDG_RUNTIME_DIR if not set */
            if (!getenv("XDG_RUNTIME_DIR")) {
                setenv("XDG_RUNTIME_DIR", runtime_dir, 1);
            }

            closedir(dir);
            return 1;
        }
    }

    closedir(dir);
    return 0;
}

/* Check if we have a Wayland display available (Wayland-only, no X11 support) */
static int has_display(uid_t uid)
{
    return find_wayland_display(uid);
}

/* Get the parent process name */
static int get_process_name(pid_t pid, char *name, size_t len)
{
    char path[64];
    char buf[256];
    FILE *f;
    
    snprintf(path, sizeof(path), "/proc/%d/comm", pid);
    f = fopen(path, "r");
    if (!f) {
        snprintf(name, len, "unknown");
        return -1;
    }
    
    if (fgets(buf, sizeof(buf), f)) {
        buf[strcspn(buf, "\n")] = '\0';
        snprintf(name, len, "%s", buf);
    } else {
        snprintf(name, len, "unknown");
    }
    
    fclose(f);
    return 0;
}

/* Get the process executable path */
static int get_process_exe(pid_t pid, char *exe, size_t len)
{
    char path[64];
    ssize_t n;
    
    snprintf(path, sizeof(path), "/proc/%d/exe", pid);
    n = readlink(path, exe, len - 1);
    if (n < 0) {
        snprintf(exe, len, "unknown");
        return -1;
    }
    
    exe[n] = '\0';
    return 0;
}

/* Parse a boolean value */
static int parse_bool(const char *val)
{
    if (!val) return 0;
    return (strcasecmp(val, "yes") == 0 || 
            strcasecmp(val, "true") == 0 ||
            strcasecmp(val, "1") == 0);
}

/* Strip whitespace from string */
static char *strip(char *s)
{
    while (*s == ' ' || *s == '\t') s++;
    size_t len = strlen(s);
    if (len == 0) return s;
    char *end = s + len - 1;
    while (end > s && (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r'))
        *end-- = '\0';
    return s;
}

/* Load configuration for a service */
static int load_config(const char *service, confirm_config_t *cfg)
{
    FILE *f;
    char line[MAX_LINE];
    char section[64] = "";
    char service_cfg[256] = "";
    int found_service = 0;
    
    /* Set defaults */
    cfg->enabled = 1;
    cfg->timeout = 30;
    cfg->randomize_buttons = 1;
    cfg->show_process_info = 1;
    cfg->log_attempts = 1;
    cfg->min_display_time = 500;
    snprintf(cfg->headless_action, sizeof(cfg->headless_action), "password");
    snprintf(cfg->title, sizeof(cfg->title), "Authentication Required");
    snprintf(cfg->message, sizeof(cfg->message),
             "The application \"%%p\" is requesting elevated privileges.");
    snprintf(cfg->secondary, sizeof(cfg->secondary),
             "Click \"Allow\" to continue or \"Deny\" to cancel.");
    
    f = fopen(CONFIG_PATH, "r");
    if (!f) {
        syslog(LOG_WARNING, "%s: cannot open config file %s: %s",
               MODULE_NAME, CONFIG_PATH, strerror(errno));
        return 0; /* Use defaults */
    }
    
    while (fgets(line, sizeof(line), f)) {
        char *l = strip(line);
        
        /* Skip comments and empty lines */
        if (*l == '#' || *l == '\0') continue;
        
        /* Section header */
        if (*l == '[') {
            char *end = strchr(l, ']');
            if (end) {
                *end = '\0';
                snprintf(section, sizeof(section), "%s", l + 1);
            }
            continue;
        }
        
        /* Key=value */
        char *eq = strchr(l, '=');
        if (!eq) continue;
        
        *eq = '\0';
        char *key = strip(l);
        char *val = strip(eq + 1);
        
        if (strcmp(section, "general") == 0) {
            if (strcmp(key, "enabled") == 0)
                cfg->enabled = parse_bool(val);
            else if (strcmp(key, "timeout") == 0)
                cfg->timeout = atoi(val);
            else if (strcmp(key, "randomize_buttons") == 0)
                cfg->randomize_buttons = parse_bool(val);
            else if (strcmp(key, "show_process_info") == 0)
                cfg->show_process_info = parse_bool(val);
            else if (strcmp(key, "log_attempts") == 0)
                cfg->log_attempts = parse_bool(val);
            else if (strcmp(key, "min_display_time") == 0)
                cfg->min_display_time = atoi(val);
            else if (strcmp(key, "headless_action") == 0)
                snprintf(cfg->headless_action, sizeof(cfg->headless_action), "%s", val);
        }
        else if (strcmp(section, "services") == 0) {
            if (strcmp(key, service) == 0) {
                snprintf(service_cfg, sizeof(service_cfg), "%s", val);
                found_service = 1;
            }
        }
        else if (strcmp(section, "appearance") == 0) {
            if (strcmp(key, "title") == 0)
                snprintf(cfg->title, sizeof(cfg->title), "%s", val);
            else if (strcmp(key, "message") == 0)
                snprintf(cfg->message, sizeof(cfg->message), "%s", val);
            else if (strcmp(key, "secondary") == 0)
                snprintf(cfg->secondary, sizeof(cfg->secondary), "%s", val);
        }
    }
    
    fclose(f);
    
    /* Parse service-specific config: enabled,timeout,randomize */
    if (found_service && *service_cfg) {
        char *copy = strdup(service_cfg);
        if (copy) {
            char *saveptr = NULL;
            int field = 0;
            char *token = strtok_r(copy, ",", &saveptr);
            while (token) {
                char *v = strip(token);
                if (strcmp(v, "default") != 0) {
                    switch (field) {
                        case 0: /* enabled */
                            cfg->enabled = parse_bool(v);
                            break;
                        case 1: /* timeout */
                            cfg->timeout = atoi(v);
                            break;
                        case 2: /* randomize */
                            cfg->randomize_buttons = parse_bool(v);
                            break;
                        default:
                            break;
                    }
                }
                token = strtok_r(NULL, ",", &saveptr);
                field++;
            }
            free(copy);
        }
    }
    
    return 0;
}

/* Format a message string with substitutions */
static void format_message(const char *template, char *output, size_t len,
                          const char *user, const char *service, const char *process)
{
    const char *p = template;
    char *o = output;
    char *end = output + len - 1;
    
    while (*p && o < end) {
        if (*p == '%' && *(p+1)) {
            p++;
            switch (*p) {
                case 'u':
                    while (*user && o < end) *o++ = *user++;
                    break;
                case 's':
                    while (*service && o < end) *o++ = *service++;
                    break;
                case 'p':
                    while (*process && o < end) *o++ = *process++;
                    break;
                case '%':
                    *o++ = '%';
                    break;
                default:
                    *o++ = '%';
                    if (o < end) *o++ = *p;
            }
            p++;
        } else {
            *o++ = *p++;
        }
    }
    *o = '\0';
}

/* Run the helper and get the result */
static int run_helper(pam_handle_t *pamh, const confirm_config_t *cfg,
                      const char *user, const char *service)
{
    pid_t pid;
    int pipefd[2];
    int status;
    char result[16];
    ssize_t n;

    (void)pamh;  /* unused */
    
    char process_name[256];
    char process_exe[512];
    char formatted_message[512];
    char formatted_secondary[512];
    
    /* Get parent process info */
    pid_t ppid = getppid();
    get_process_name(ppid, process_name, sizeof(process_name));
    get_process_exe(ppid, process_exe, sizeof(process_exe));
    
    /* Format messages */
    format_message(cfg->message, formatted_message, sizeof(formatted_message),
                   user, service, process_name);
    format_message(cfg->secondary, formatted_secondary, sizeof(formatted_secondary),
                   user, service, process_name);
    
    if (pipe(pipefd) < 0) {
        syslog(LOG_ERR, "%s: pipe() failed: %s", MODULE_NAME, strerror(errno));
        return PAM_SYSTEM_ERR;
    }
    
    pid = fork();
    if (pid < 0) {
        syslog(LOG_ERR, "%s: fork() failed: %s", MODULE_NAME, strerror(errno));
        close(pipefd[0]);
        close(pipefd[1]);
        return PAM_SYSTEM_ERR;
    }
    
    if (pid == 0) {
        /* Child process - run helper */
        close(pipefd[0]);
        
        /* Drop privileges to the calling user */
        const char *sudo_uid = getenv("SUDO_UID");
        const char *pkexec_uid = getenv("PKEXEC_UID");
        uid_t target_uid = getuid();
        gid_t target_gid = getgid();
        
        /* Try to get the original user's UID */
        if (sudo_uid) {
            target_uid = (uid_t)atoi(sudo_uid);
        } else if (pkexec_uid) {
            target_uid = (uid_t)atoi(pkexec_uid);
        } else {
            /* Fall back to PAM_RUSER or lookup the user */
            struct passwd *pw = getpwnam(user);
            if (pw) {
                target_uid = pw->pw_uid;
                target_gid = pw->pw_gid;
            }
        }
        
        /* Get the user's gid and set up environment */
        struct passwd *pw = getpwuid(target_uid);
        if (pw) {
            target_gid = pw->pw_gid;
            setenv("HOME", pw->pw_dir, 1);
            setenv("USER", pw->pw_name, 1);
            setenv("LOGNAME", pw->pw_name, 1);

            /* Set supplementary groups */
            if (initgroups(pw->pw_name, target_gid) < 0) {
                _exit(1);
            }
        }

        /* Drop privileges: must set gid first, then uid */
        if (setgid(target_gid) < 0) {
            _exit(1);
        }
        if (setuid(target_uid) < 0) {
            _exit(1);
        }

        /* Redirect stdout to pipe */
        if (dup2(pipefd[1], STDOUT_FILENO) < 0) {
            _exit(1);
        }
        close(pipefd[1]);
        
        /* Build argument list */
        char timeout_str[16];
        char min_time_str[16];
        snprintf(timeout_str, sizeof(timeout_str), "%d", cfg->timeout);
        snprintf(min_time_str, sizeof(min_time_str), "%d", cfg->min_display_time);
        
        char *args[20];
        int argc = 0;
        
        args[argc++] = (char *)HELPER_PATH;
        args[argc++] = "--title";
        args[argc++] = (char *)cfg->title;
        args[argc++] = "--message";
        args[argc++] = formatted_message;
        args[argc++] = "--secondary";
        args[argc++] = formatted_secondary;
        args[argc++] = "--timeout";
        args[argc++] = timeout_str;
        args[argc++] = "--min-time";
        args[argc++] = min_time_str;
        
        if (cfg->randomize_buttons)
            args[argc++] = "--randomize";
        if (cfg->show_process_info) {
            args[argc++] = "--process-exe";
            args[argc++] = process_exe;
        }
        
        args[argc] = NULL;
        
        execv(HELPER_PATH, args);

        /* If we get here, exec failed - write to stdout (the pipe) */
        if (write(STDOUT_FILENO, "DENY\n", 5) < 0) {
            /* Nothing we can do, exiting anyway */
        }
        _exit(1);
    }
    
    /* Parent process */
    close(pipefd[1]);
    
    /* Read result with timeout */
    struct timeval tv;
    fd_set fds;
    tv.tv_sec = cfg->timeout + 5;  /* Give helper time + buffer */
    tv.tv_usec = 0;
    
    FD_ZERO(&fds);
    FD_SET(pipefd[0], &fds);
    
    int sel = select(pipefd[0] + 1, &fds, NULL, NULL, &tv);
    
    if (sel <= 0) {
        /* Timeout or error */
        kill(pid, SIGKILL);
        waitpid(pid, &status, 0);
        close(pipefd[0]);
        
        if (cfg->log_attempts)
            syslog(LOG_WARNING, "%s: helper timeout for user %s, service %s",
                   MODULE_NAME, user, service);
        return PAM_AUTH_ERR;
    }
    
    n = read(pipefd[0], result, sizeof(result) - 1);
    close(pipefd[0]);
    
    waitpid(pid, &status, 0);
    
    if (n <= 0) {
        if (cfg->log_attempts)
            syslog(LOG_WARNING, "%s: helper returned no result for user %s, service %s",
                   MODULE_NAME, user, service);
        return PAM_AUTH_ERR;
    }
    
    result[n] = '\0';
    result[strcspn(result, "\n\r")] = '\0';
    
    if (cfg->log_attempts) {
        syslog(LOG_INFO, "%s: user %s, service %s, process %s: %s",
               MODULE_NAME, user, service, process_name, result);
    }
    
    if (strcmp(result, "ALLOW") == 0) {
        return PAM_SUCCESS;
    }
    
    return PAM_AUTH_ERR;
}

/* Main PAM entry point */
PAM_EXTERN int pam_sm_authenticate(pam_handle_t *pamh, int flags,
                                   int argc, const char **argv)
{
    const char *user = NULL;
    const char *service = NULL;
    confirm_config_t cfg;
    int ret;
    
    (void)flags;
    (void)argc;
    (void)argv;

    /* Get username */
    ret = pam_get_user(pamh, &user, NULL);
    if (ret != PAM_SUCCESS || !user) {
        syslog(LOG_ERR, "%s: cannot get username", MODULE_NAME);
        return PAM_USER_UNKNOWN;
    }
    
    /* Get service name */
    ret = pam_get_item(pamh, PAM_SERVICE, (const void **)&service);
    if (ret != PAM_SUCCESS || !service) {
        service = "unknown";
    }
    
    /* Load configuration */
    load_config(service, &cfg);
    
    /* Check if enabled for this service */
    if (!cfg.enabled) {
        syslog(LOG_DEBUG, "%s: disabled for service %s", MODULE_NAME, service);
        return PAM_IGNORE;
    }
    
    /* Get user's UID for display detection */
    struct passwd *pw = getpwnam(user);
    uid_t target_uid = pw ? pw->pw_uid : getuid();

    /* Check for graphical display */
    if (!has_display(target_uid)) {
        if (strcmp(cfg.headless_action, "allow") == 0) {
            syslog(LOG_WARNING, "%s: no display, allowing (service %s, user %s)",
                   MODULE_NAME, service, user);
            return PAM_SUCCESS;
        }
        else if (strcmp(cfg.headless_action, "deny") == 0) {
            syslog(LOG_INFO, "%s: no display, denying (service %s, user %s)",
                   MODULE_NAME, service, user);
            return PAM_AUTH_ERR;
        }
        else {
            /* password - let other modules handle it */
            syslog(LOG_DEBUG, "%s: no display, falling back to password (service %s)",
                   MODULE_NAME, service);
            return PAM_IGNORE;
        }
    }
    
    /* Run the graphical helper */
    return run_helper(pamh, &cfg, user, service);
}

PAM_EXTERN int pam_sm_setcred(pam_handle_t *pamh, int flags,
                              int argc, const char **argv)
{
    (void)pamh;
    (void)flags;
    (void)argc;
    (void)argv;
    return PAM_SUCCESS;
}
