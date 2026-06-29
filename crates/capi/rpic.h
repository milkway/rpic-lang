/* rpic — stable C ABI.
 *
 * String results are heap-allocated, NUL-terminated UTF-8 (free with
 * rpic_free_string) or NULL on error. Byte results write their length to
 * out_len and return a heap buffer (free with rpic_free_bytes) or NULL.
 * `circuits` (0/1) prepends the native circuit-element library. */
#ifndef RPIC_H
#define RPIC_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

char *rpic_render_svg(const char *src, int circuits);
char *rpic_compile_json(const char *src, int circuits);
unsigned char *rpic_render_png(const char *src, double scale, int circuits, size_t *out_len);
unsigned char *rpic_render_pdf(const char *src, int circuits, size_t *out_len);
void rpic_free_string(char *p);
void rpic_free_bytes(unsigned char *p, size_t len);

#ifdef __cplusplus
}
#endif

#endif /* RPIC_H */
