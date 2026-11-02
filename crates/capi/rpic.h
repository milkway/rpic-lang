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

/* Full compile options (the `_ex` entry points). Zeroed = circuits-off
 * defaults. include_policy: 0 = unrestricted (CLI default), 1 = sandboxed to
 * `base`, 2 = deny all filesystem includes. `base` (or NULL) is the directory
 * `copy "file"` resolves against. Embedders compiling untrusted source should
 * set policy 1 or 2. */
typedef struct {
  int circuits;
  int texlabels;
  int include_policy;
  const char *base;
} RpicOptions;

char *rpic_render_svg_ex(const char *src, const RpicOptions *opts);
char *rpic_compile_json_ex(const char *src, const RpicOptions *opts);
/* The device-independent scene draw list (docs/scene.md) alone. flatten > 0
 * delivers curves as polylines within that tolerance in inches; 0 keeps the
 * cubic Béziers. NULL on error; free with rpic_free_string. */
char *rpic_scene_json(const char *src, const RpicOptions *opts, double flatten);
/* The compile bundle with the "scene" key included (scene warnings merged
 * into "warnings"). flatten as in rpic_scene_json. */
char *rpic_compile_json_all(const char *src, const RpicOptions *opts, double flatten);
unsigned char *rpic_render_png_ex(const char *src, double scale, const RpicOptions *opts, size_t *out_len);
unsigned char *rpic_render_pdf_ex(const char *src, const RpicOptions *opts, size_t *out_len);

#ifdef __cplusplus
}
#endif

#endif /* RPIC_H */
