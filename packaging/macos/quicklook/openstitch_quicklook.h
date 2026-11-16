// C declarations of the Rust renderer the Quick Look extension links (render/src/lib.rs, #1182).
// Keep in step with that file.
#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint8_t *ptr;
    size_t len;
} OpenStitchBuffer;

// 0: *out holds the PNG; 1: *out holds a UTF-8 error message. Free *out with openstitch_ql_free.
int32_t openstitch_ql_thumbnail(const uint8_t *data, size_t len, const char *extension,
                                uint32_t size, OpenStitchBuffer *out);
void openstitch_ql_free(OpenStitchBuffer buffer);
