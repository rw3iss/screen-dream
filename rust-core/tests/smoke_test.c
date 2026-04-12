/**
 * Screen Dream FFI — C smoke test.
 *
 * Verifies that the generated C header is usable and the basic
 * init / platform-info / shutdown lifecycle works from C.
 *
 * Build & run:
 *   cd rust-core
 *   cargo build -p ffi
 *   gcc tests/smoke_test.c -o tests/smoke_test -L target/debug -lscreen_dream_ffi -Icrates/ffi
 *   LD_LIBRARY_PATH=target/debug ./tests/smoke_test
 */

#include "../crates/ffi/screen_dream_ffi.h"
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    SDError *err = NULL;

    /* Initialize the core. */
    bool ok = sd_init("/tmp/screen-dream-test", &err);
    if (!ok) {
        if (err) {
            printf("Init failed: %s\n", err->message);
            sd_free_error(err);
        } else {
            printf("Init failed (no error details)\n");
        }
        return 1;
    }

    /* Query platform info. */
    SDPlatformInfo info = sd_get_platform_info();
    printf("Platform: %s / %s / %s\n", info.os, info.display_server, info.arch);
    sd_free_string(info.os);
    sd_free_string(info.display_server);
    sd_free_string(info.arch);

    /* Shut down. */
    sd_shutdown();

    printf("Smoke test passed!\n");
    return 0;
}
