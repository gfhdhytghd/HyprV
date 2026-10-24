// Accessors so Rust does not mirror struct cava_plan's layout (vendor stays unmodified).
#include "../vendor/cavacore/cavacore.h"

int hyprv_cava_status(const struct cava_plan *plan) { return plan->status; }
const char *hyprv_cava_error(const struct cava_plan *plan) { return plan->error_message; }
int hyprv_cava_input_buffer_size(const struct cava_plan *plan) { return plan->input_buffer_size; }
