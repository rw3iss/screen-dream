// DRM framebuffer screenshot — captures a monitor and saves directly to PNG.
// Handles AB30 (10-bit ABGR) and AB4H (FP16 half-float) pixel formats.
// Requires: CAP_SYS_ADMIN, libdrm, libpng
// Build: gcc drm_screenshot.c -o drm_screenshot -ldrm -lpng -lm -I/usr/include/libdrm
// Usage: drm_screenshot [output.png] [plane_id]
//   Without args: captures first active plane, saves to /tmp/drm_screenshot.png
//   drm_screenshot --list: list active planes

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/mman.h>
#include <stdint.h>
#include <math.h>
#include <time.h>
#include <xf86drm.h>
#include <xf86drmMode.h>
#include <png.h>

// FP16 half-float to float conversion
static float half_to_float(uint16_t h) {
    uint32_t sign = (h >> 15) & 1;
    uint32_t exp = (h >> 10) & 0x1F;
    uint32_t mant = h & 0x3FF;

    if (exp == 0) {
        if (mant == 0) return sign ? -0.0f : 0.0f;
        // Denormalized
        float f = (mant / 1024.0f) * powf(2.0f, -14.0f);
        return sign ? -f : f;
    }
    if (exp == 31) {
        if (mant == 0) return sign ? -INFINITY : INFINITY;
        return NAN;
    }
    float f = powf(2.0f, (float)(exp - 15)) * (1.0f + mant / 1024.0f);
    return sign ? -f : f;
}

static uint8_t clamp_u8(float v) {
    if (v <= 0.0f) return 0;
    if (v >= 1.0f) return 255;
    return (uint8_t)(v * 255.0f + 0.5f);
}

typedef struct {
    int fd;
    uint32_t plane_id;
    uint32_t crtc_id;
    uint32_t fb_id;
    uint32_t width;
    uint32_t height;
    uint32_t pitch;
    uint32_t format;
    uint32_t handle;
} PlaneInfo;

static int find_active_planes(int fd, PlaneInfo *planes, int max_planes) {
    drmSetClientCap(fd, DRM_CLIENT_CAP_UNIVERSAL_PLANES, 1);
    drmModePlaneResPtr res = drmModeGetPlaneResources(fd);
    if (!res) return 0;

    int count = 0;
    for (unsigned i = 0; i < res->count_planes && count < max_planes; i++) {
        drmModePlanePtr p = drmModeGetPlane(fd, res->planes[i]);
        if (!p || p->fb_id == 0 || p->crtc_id == 0) { if (p) drmModeFreePlane(p); continue; }

        drmModeFB2Ptr fb = drmModeGetFB2(fd, p->fb_id);
        if (!fb || fb->handles[0] == 0) { if (fb) drmModeFreeFB2(fb); drmModeFreePlane(p); continue; }

        planes[count].fd = fd;
        planes[count].plane_id = p->plane_id;
        planes[count].crtc_id = p->crtc_id;
        planes[count].fb_id = p->fb_id;
        planes[count].width = fb->width;
        planes[count].height = fb->height;
        planes[count].pitch = fb->pitches[0];
        planes[count].format = fb->pixel_format;
        planes[count].handle = fb->handles[0];
        count++;

        drmModeFreeFB2(fb);
        drmModeFreePlane(p);
    }
    drmModeFreePlaneResources(res);
    return count;
}

static int capture_to_png(PlaneInfo *plane, const char *output_path) {
    struct timespec t0, t1, t2, t3;
    clock_gettime(CLOCK_MONOTONIC, &t0);

    // Export handle to DMA-BUF fd
    int prime_fd = -1;
    if (drmPrimeHandleToFD(plane->fd, plane->handle, O_RDONLY, &prime_fd) != 0) {
        fprintf(stderr, "drmPrimeHandleToFD failed\n");
        return 1;
    }

    // Map the DMA-BUF
    size_t size = (size_t)plane->pitch * plane->height;
    void *map = mmap(NULL, size, PROT_READ, MAP_SHARED, prime_fd, 0);
    if (map == MAP_FAILED) {
        fprintf(stderr, "mmap failed (size=%zu)\n", size);
        close(prime_fd);
        return 1;
    }

    clock_gettime(CLOCK_MONOTONIC, &t1);
    double map_ms = (t1.tv_sec - t0.tv_sec) * 1000.0 + (t1.tv_nsec - t0.tv_nsec) / 1e6;

    uint32_t w = plane->width;
    uint32_t h = plane->height;
    uint32_t fmt = plane->format;
    char fmt_str[5] = {0};
    memcpy(fmt_str, &fmt, 4);

    printf("Captured %ux%u format=%s pitch=%u mmap=%.1fms\n", w, h, fmt_str, plane->pitch, map_ms);

    // Copy from GPU memory to RAM first (much faster to process from RAM)
    uint8_t *local = malloc(size);
    if (!local) { munmap(map, size); close(prime_fd); return 1; }
    memcpy(local, map, size);
    munmap(map, size);
    close(prime_fd);

    struct timespec t_copy;
    clock_gettime(CLOCK_MONOTONIC, &t_copy);
    double copy_ms = (t_copy.tv_sec - t1.tv_sec) * 1000.0 + (t_copy.tv_nsec - t1.tv_nsec) / 1e6;
    printf("GPU->RAM copy: %.1fms (%.1f MB/s)\n", copy_ms, size / 1024.0 / 1024.0 / (copy_ms / 1000.0));

    // Convert pixels to 8-bit RGBA
    uint8_t *rgba = malloc(w * h * 4);
    if (!rgba) { free(local); return 1; }

    const uint8_t *src = local;

    // AB30 = ABGR 2-10-10-10 (little-endian packed)
    // AB4H = ABGR FP16 half-float (8 bytes per pixel: R16 G16 B16 A16)
    // XR24 = XRGB 8888
    // AR24 = ARGB 8888

    if (fmt == 0x30334241 /* AB30 */) {
        // 10-bit ABGR: 32 bits per pixel
        // Little-endian: bits [9:0]=B, [19:10]=G, [29:20]=R, [31:30]=A
        for (uint32_t y = 0; y < h; y++) {
            const uint32_t *row = (const uint32_t *)(src + y * plane->pitch);
            uint8_t *dst = rgba + y * w * 4;
            for (uint32_t x = 0; x < w; x++) {
                uint32_t px = row[x];
                uint32_t b10 = (px >>  0) & 0x3FF;
                uint32_t g10 = (px >> 10) & 0x3FF;
                uint32_t r10 = (px >> 20) & 0x3FF;
                // Convert 10-bit to 8-bit
                dst[x*4 + 0] = r10 >> 2;
                dst[x*4 + 1] = g10 >> 2;
                dst[x*4 + 2] = b10 >> 2;
                dst[x*4 + 3] = 255;
            }
        }
    } else if (fmt == 0x48344241 /* AB4H */) {
        // FP16 ABGR: 8 bytes per pixel (R16 G16 B16 A16 in ABGR order)
        // Actually the memory layout is: B16, G16, R16, A16 (ABGR = alpha last)
        for (uint32_t y = 0; y < h; y++) {
            const uint16_t *row = (const uint16_t *)(src + y * plane->pitch);
            uint8_t *dst = rgba + y * w * 4;
            for (uint32_t x = 0; x < w; x++) {
                // ABGR FP16: memory order is R, G, B, A (little-endian halfwords)
                // Actually for DRM ABGR: byte order in memory depends on endianness
                // Let's try: halfwords at offsets 0=R, 1=G, 2=B, 3=A
                float r = half_to_float(row[x*4 + 0]);
                float g = half_to_float(row[x*4 + 1]);
                float b = half_to_float(row[x*4 + 2]);
                dst[x*4 + 0] = clamp_u8(r);
                dst[x*4 + 1] = clamp_u8(g);
                dst[x*4 + 2] = clamp_u8(b);
                dst[x*4 + 3] = 255;
            }
        }
    } else if (fmt == 0x34325258 /* XR24 */ || fmt == 0x34325241 /* AR24 */) {
        // 8-bit XRGB/ARGB: memory order B, G, R, A/X
        for (uint32_t y = 0; y < h; y++) {
            const uint8_t *row = src + y * plane->pitch;
            uint8_t *dst = rgba + y * w * 4;
            for (uint32_t x = 0; x < w; x++) {
                dst[x*4 + 0] = row[x*4 + 2]; // R
                dst[x*4 + 1] = row[x*4 + 1]; // G
                dst[x*4 + 2] = row[x*4 + 0]; // B
                dst[x*4 + 3] = 255;
            }
        }
    } else {
        fprintf(stderr, "Unsupported pixel format: %s (0x%08X)\n", fmt_str, fmt);
        free(rgba);
        munmap(map, size);
        close(prime_fd);
        return 1;
    }

    clock_gettime(CLOCK_MONOTONIC, &t2);
    double convert_ms = (t2.tv_sec - t1.tv_sec) * 1000.0 + (t2.tv_nsec - t1.tv_nsec) / 1e6;

    // Write PNG
    FILE *fp = fopen(output_path, "wb");
    if (!fp) { fprintf(stderr, "Cannot open %s\n", output_path); free(rgba); munmap(map, size); close(prime_fd); return 1; }

    png_structp png = png_create_write_struct(PNG_LIBPNG_VER_STRING, NULL, NULL, NULL);
    png_infop info = png_create_info_struct(png);
    png_init_io(png, fp);
    png_set_IHDR(png, info, w, h, 8, PNG_COLOR_TYPE_RGBA,
                 PNG_INTERLACE_NONE, PNG_COMPRESSION_TYPE_DEFAULT, PNG_FILTER_TYPE_DEFAULT);
    // Fast compression (speed over size)
    png_set_compression_level(png, 1);
    png_write_info(png, info);

    png_bytep *rows = malloc(h * sizeof(png_bytep));
    for (uint32_t y = 0; y < h; y++)
        rows[y] = rgba + y * w * 4;
    png_write_image(png, rows);
    png_write_end(png, NULL);

    fclose(fp);
    free(rows);

    clock_gettime(CLOCK_MONOTONIC, &t3);
    double png_ms = (t3.tv_sec - t2.tv_sec) * 1000.0 + (t3.tv_nsec - t2.tv_nsec) / 1e6;
    double total_ms = (t3.tv_sec - t0.tv_sec) * 1000.0 + (t3.tv_nsec - t0.tv_nsec) / 1e6;

    printf("Timing: mmap=%.1fms convert=%.1fms png=%.1fms total=%.1fms\n",
           map_ms, convert_ms, png_ms, total_ms);
    printf("Saved: %s\n", output_path);

    free(rgba);
    free(local);
    return 0;
}

int main(int argc, char **argv) {
    // Try both cards
    const char *cards[] = {"/dev/dri/card1", "/dev/dri/card2"};
    PlaneInfo planes[10];
    int total_planes = 0;
    int active_fd = -1;

    for (int c = 0; c < 2; c++) {
        int fd = open(cards[c], O_RDWR);
        if (fd < 0) continue;
        int n = find_active_planes(fd, planes + total_planes, 10 - total_planes);
        if (n > 0) {
            active_fd = fd;
            total_planes += n;
        } else {
            close(fd);
        }
    }

    if (total_planes == 0) {
        fprintf(stderr, "No active framebuffers found. Need CAP_SYS_ADMIN.\n");
        return 1;
    }

    if (argc > 1 && strcmp(argv[1], "--list") == 0) {
        for (int i = 0; i < total_planes; i++) {
            char fmt[5] = {0};
            memcpy(fmt, &planes[i].format, 4);
            printf("Plane %u: CRTC %u, %ux%u, format=%s, pitch=%u\n",
                   planes[i].plane_id, planes[i].crtc_id,
                   planes[i].width, planes[i].height, fmt, planes[i].pitch);
        }
        close(active_fd);
        return 0;
    }

    // Which plane to capture?
    int target = 0; // default: first
    const char *output = "/tmp/drm_screenshot.png";

    if (argc > 1) output = argv[1];
    if (argc > 2) {
        uint32_t pid = atoi(argv[2]);
        for (int i = 0; i < total_planes; i++) {
            if (planes[i].plane_id == pid) { target = i; break; }
        }
    }

    int ret = capture_to_png(&planes[target], output);
    close(active_fd);
    return ret;
}
