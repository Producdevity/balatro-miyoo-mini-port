#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/prctl.h>
#include <sys/soundcard.h>
#include <sys/stat.h>
#include <unistd.h>

static volatile sig_atomic_t stopping;

static void stop(int signal_number) {
    (void)signal_number;
    stopping = 1;
}

static int configure(int fd, unsigned long request, int value) {
    int actual = value;
    if (ioctl(fd, request, &actual) < 0 || actual != value) {
        fprintf(stderr, "[audio] output rejected request %lx: wanted %d, got %d\n",
                request, value, actual);
        return -1;
    }
    return 0;
}

int main(void) {
    struct sigaction action = {.sa_handler = stop};
    sigemptyset(&action.sa_mask);
    sigaction(SIGTERM, &action, NULL);
    sigaction(SIGINT, &action, NULL);
    sigaction(SIGPIPE, &action, NULL);
    prctl(PR_SET_PDEATHSIG, SIGTERM);
    if (getppid() == 1) return 1;

    /* Onion's interposer needs its real open target before the first DSP open. */
    int warmup = open("/dev/null", O_RDONLY);
    if (warmup < 0) return 1;
    close(warmup);
    int fd = open("/dev/dsp", O_WRONLY);
    if (fd < 0) { perror("[audio] open output"); return 1; }
    int result = 1;
    struct stat st;
    if (fstat(fd, &st) < 0 || !S_ISFIFO(st.st_mode)) {
        fprintf(stderr, "[audio] Onion audio server is not active\n");
        goto done;
    }
    if (fcntl(fd, F_SETPIPE_SZ, 4096) < 0 || fcntl(fd, F_GETPIPE_SZ) > 4096) {
        perror("[audio] limit output queue");
        goto done;
    }
    if (configure(fd, SNDCTL_DSP_SETFMT, AFMT_S16_LE) < 0 ||
        configure(fd, SNDCTL_DSP_CHANNELS, 2) < 0 ||
        configure(fd, SNDCTL_DSP_SPEED, 44100) < 0) goto done;
    int flags = fcntl(fd, F_GETFL);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0) goto done;
    fprintf(stderr, "[audio] Onion server: 44100 Hz stereo, 4096-byte queue\n");

    unsigned char buffer[1024];
    while (!stopping) {
        ssize_t count = read(STDIN_FILENO, buffer, sizeof(buffer));
        if (count == 0) { result = 0; break; }
        if (count < 0) {
            if (errno == EINTR) continue;
            perror("[audio] read mixer");
            break;
        }
        ssize_t offset = 0;
        while (offset < count && !stopping) {
            ssize_t written = write(fd, buffer + offset, count - offset);
            if (written > 0) { offset += written; continue; }
            if (written < 0 && errno == EINTR) continue;
            if (written < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
                struct pollfd output = {.fd = fd, .events = POLLOUT};
                int ready = poll(&output, 1, 1000);
                if (ready > 0 && !(output.revents & (POLLERR | POLLHUP | POLLNVAL))) continue;
                if (ready < 0 && errno == EINTR) continue;
                fprintf(stderr, "[audio] output stopped accepting samples\n");
            } else {
                perror("[audio] write output");
            }
            goto done;
        }
    }
done:;
    /* The interposer always consumes the third ioctl argument, even for reset. */
    int reset = 0;
    ioctl(fd, SNDCTL_DSP_RESET, &reset);
    close(fd);
    return result;
}
