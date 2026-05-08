/*
 * Cobrust M11 runtime entry shim (per ADR-0025 §G).
 *
 * The platform's C runtime (crt0) calls `int main(int argc, char**
 * argv)`. This shim:
 *   1. Captures argc/argv into the cobrust-stdlib runtime's CAPTURED_ARGS
 *      buffer, so std.env.args() returns them.
 *   2. Invokes `_cobrust_user_main()` — the codegen-emitted user main.
 *      At M11 the user main is `fn main() -> i64`; the i64 return value
 *      is the process exit code.
 *
 * M12 will widen the user-main signature to `fn main() -> Result<(), Error>`.
 * Until then, code 0 = success; codes 3 (panic) and 4 (runtime panic) come
 * from the panic handler exiting directly.
 *
 * ADR-0025 §"Runtime ABI" pins the symbol contract.
 */

#include <stdint.h>

extern void __cobrust_capture_argv(int argc, const char* const* argv);
extern int64_t _cobrust_user_main(void);

int main(int argc, char** argv) {
    __cobrust_capture_argv(argc, (const char* const*) argv);
    int64_t rc = _cobrust_user_main();
    /* Truncate to the C int width per platform; user-main's i64
     * return is the user's value. */
    return (int) rc;
}
