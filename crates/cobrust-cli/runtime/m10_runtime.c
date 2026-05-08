/*
 * Cobrust M10 runtime helper (per ADR-0024 §"Hello-world contract").
 *
 * M10 ships a single intrinsic, `__cobrust_println_static`, that prints
 * the literal "hello, world\n" to stdout. The CLI's `build` subcommand
 * recognizes `print("hello, world")` callsites in the user's `.cb` source
 * and rewrites the MIR Call to invoke this symbol. The honesty audit
 * (ADR-0024 §"Hello-world mechanism" option 3) requires the user's `.cb`
 * source to contain the literal `"hello, world"` string; the CLI rejects
 * any other argument with an explicit M11-narrowing diagnostic.
 *
 * M11 stdlib (`std.io.println`) supersedes by lifting string emission
 * into the codegen path with a real `(*const u8, usize)` runtime ABI.
 * At that point this file is removed.
 */

#include <unistd.h>

void __cobrust_println_static(void) {
    static const char msg[] = "hello, world\n";
    /* write(2) of a fixed-length payload — no allocation, no errno
     * handling at M10. ADR-0023 deferred panic infrastructure to M11.
     */
    (void) write(1 /* STDOUT_FILENO */, msg, sizeof(msg) - 1);
}
