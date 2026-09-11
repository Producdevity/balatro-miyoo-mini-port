#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <sys/soundcard.h>
#include <time.h>
#include <unistd.h>

static int output_fd = -1, read_fd = -1, capture_fd = -1;
static int (*real_open)(const char *, int, ...);
static ssize_t (*real_write)(int, const void *, size_t);
static pthread_t reader;
static unsigned calls;

static void *capture(void *unused) {
    (void)unused;
    char data[256];
    ssize_t count;
    size_t total = 0;
    const char *cutoff = getenv("TEST_CUTOFF");
    while ((count = read(read_fd, data, sizeof(data))) > 0) {
        if (real_write(capture_fd, data, (size_t)count) != count) abort();
        total += (size_t)count;
        if (cutoff && total >= strtoul(cutoff, NULL, 10)) break;
        struct timespec delay = {.tv_nsec = count * 1000000000LL / 176400};
        nanosleep(&delay, NULL);
    }
    close(read_fd);
    close(capture_fd);
    return NULL;
}

int open(const char *path, int flags, ...) {
    if (!real_open) {
        real_open = dlsym(RTLD_NEXT, "open");
        real_write = dlsym(RTLD_NEXT, "write");
    }
    if (strcmp(path,"/dev/dsp") == 0) {
        int pipe_fd[2];
        if (pipe(pipe_fd) < 0) return -1;
        read_fd = pipe_fd[0]; output_fd = pipe_fd[1];
        capture_fd = real_open(getenv("TEST_CAPTURE"),O_CREAT|O_TRUNC|O_WRONLY,0600);
        if (capture_fd < 0 || pthread_create(&reader,NULL,capture,NULL)) abort();
        return output_fd;
    }
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args; va_start(args, flags); mode = va_arg(args, int); va_end(args);
    }
    return real_open(path, flags, mode);
}

int ioctl(int fd, unsigned long request, ...) {
    if (fd != output_fd) { errno = ENOTTY; return -1; }
    va_list args; va_start(args,request); int *value = va_arg(args,int *); va_end(args);
    if (!value) abort();
    switch (request) {
        case SNDCTL_DSP_SETFMT: return *value == AFMT_S16_LE ? 0 : -1;
        case SNDCTL_DSP_CHANNELS:
            if (getenv("TEST_REJECT")) { *value = 1; return 0; }
            return *value == 2 ? 0 : -1;
        case SNDCTL_DSP_SPEED: return *value == 44100 ? 0 : -1;
        case SNDCTL_DSP_RESET: return 0;
        default: errno = EINVAL; return -1;
    }
}

ssize_t write(int fd, const void *data, size_t count) {
    if (!real_write) real_write = dlsym(RTLD_NEXT,"write");
    if (fd == output_fd) {
        calls++;
        if (calls % 7 == 0) { errno = EINTR; return -1; }
        if (calls % 11 == 0) { errno = EAGAIN; return -1; }
        if (count > 128) count = 128;
    }
    return real_write(fd,data,count);
}

__attribute__((destructor)) static void finish(void) {
    if (output_fd >= 0) pthread_join(reader,NULL);
}
