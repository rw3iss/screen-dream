// DRM framebuffer capture helper — requires CAP_SYS_ADMIN
// Usage: drm_capture_helper <card_path> <crtc_id>
// Outputs: 8 bytes header (width:u32 + height:u32) + raw BGRA pixel data to stdout

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/mman.h>
#include <stdint.h>
#include <xf86drm.h>
#include <xf86drmMode.h>

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "Usage: %s <card_path> <plane_id>\n", argv[0]);
        fprintf(stderr, "  or:  %s --list <card_path>\n", argv[0]);
        return 1;
    }

    // List mode: enumerate active planes
    if (strcmp(argv[1], "--list") == 0) {
        const char *card = argv[2];
        int fd = open(card, O_RDWR);
        if (fd < 0) { fprintf(stderr, "Cannot open %s\n", card); return 1; }

        drmSetClientCap(fd, DRM_CLIENT_CAP_UNIVERSAL_PLANES, 1);
        drmModePlaneResPtr planes = drmModeGetPlaneResources(fd);
        if (!planes) { close(fd); return 1; }

        for (unsigned i = 0; i < planes->count_planes; i++) {
            drmModePlanePtr p = drmModeGetPlane(fd, planes->planes[i]);
            if (!p || p->fb_id == 0 || p->crtc_id == 0) { if (p) drmModeFreePlane(p); continue; }
            
            drmModeFB2Ptr fb = drmModeGetFB2(fd, p->fb_id);
            if (fb && fb->handles[0] != 0) {
                // Get CRTC position
                drmModeCrtcPtr crtc = drmModeGetCrtc(fd, p->crtc_id);
                int cx = crtc ? (int)crtc->x : 0;
                int cy = crtc ? (int)crtc->y : 0;
                if (crtc) drmModeFreeCrtc(crtc);
                
                printf("PLANE:%u:CRTC:%u:FB:%u:SIZE:%ux%u:POS:%d,%d:FMT:%.4s\n",
                    p->plane_id, p->crtc_id, p->fb_id,
                    fb->width, fb->height, cx, cy, (char*)&fb->pixel_format);
                drmModeFreeFB2(fb);
            } else if (fb) { drmModeFreeFB2(fb); }
            drmModeFreePlane(p);
        }
        drmModeFreePlaneResources(planes);
        close(fd);
        return 0;
    }

    // Capture mode: read framebuffer for a specific plane
    const char *card = argv[1];
    uint32_t target_plane = (uint32_t)atoi(argv[2]);

    int fd = open(card, O_RDWR);
    if (fd < 0) { fprintf(stderr, "Cannot open %s\n", card); return 1; }

    drmSetClientCap(fd, DRM_CLIENT_CAP_UNIVERSAL_PLANES, 1);
    
    drmModePlanePtr plane = drmModeGetPlane(fd, target_plane);
    if (!plane || plane->fb_id == 0) {
        fprintf(stderr, "Plane %u not found or no framebuffer\n", target_plane);
        close(fd);
        return 1;
    }

    drmModeFB2Ptr fb = drmModeGetFB2(fd, plane->fb_id);
    if (!fb || fb->handles[0] == 0) {
        fprintf(stderr, "Cannot get FB2 for fb %u (need CAP_SYS_ADMIN)\n", plane->fb_id);
        drmModeFreePlane(plane);
        close(fd);
        return 1;
    }

    uint32_t w = fb->width;
    uint32_t h = fb->height;
    uint32_t pitch = fb->pitches[0];
    uint32_t handle = fb->handles[0];

    // Export handle to DMA-BUF fd
    int prime_fd = -1;
    if (drmPrimeHandleToFD(fd, handle, O_RDONLY, &prime_fd) != 0) {
        fprintf(stderr, "drmPrimeHandleToFD failed\n");
        drmModeFreeFB2(fb);
        drmModeFreePlane(plane);
        close(fd);
        return 1;
    }

    // Map the DMA-BUF
    size_t size = (size_t)pitch * h;
    void *map = mmap(NULL, size, PROT_READ, MAP_SHARED, prime_fd, 0);
    if (map == MAP_FAILED) {
        fprintf(stderr, "mmap failed (tiled buffer? size=%zu)\n", size);
        close(prime_fd);
        drmModeFreeFB2(fb);
        drmModeFreePlane(plane);
        close(fd);
        return 1;
    }

    // Write header: width(u32) + height(u32) + pitch(u32)
    fwrite(&w, 4, 1, stdout);
    fwrite(&h, 4, 1, stdout);
    fwrite(&pitch, 4, 1, stdout);

    // Write raw pixel data (BGRA/XRGB, with stride)
    fwrite(map, 1, size, stdout);
    fflush(stdout);

    munmap(map, size);
    close(prime_fd);
    drmModeFreeFB2(fb);
    drmModeFreePlane(plane);
    close(fd);
    return 0;
}
