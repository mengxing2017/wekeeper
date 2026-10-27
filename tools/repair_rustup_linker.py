"""Make Nix rustup's project-local LLD wrapper survive a checkout move.

Nix wraps the downloaded linker with an absolute PROG path. Only repair the
known wrapper in this checkout's Devbox cache; leave external toolchains alone.
"""

from pathlib import Path
import re


RELATIVE_LINKER = (
    'export PROG="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/'
    '../gcc-ld-unwrapped" && pwd)/ld.lld"'
)
ABSOLUTE_LINKER = re.compile(
    r'^export PROG="/[^\n"]*/gcc-ld-unwrapped/ld\.lld"$', re.MULTILINE
)


def repair(project: Path) -> int:
    toolchains = project / '.devbox/virtenv/rustup/toolchains'
    repaired = 0
    for wrapper in toolchains.glob('*/lib/rustlib/*/bin/gcc-ld/ld.lld'):
        target = wrapper.parent.parent / 'gcc-ld-unwrapped/ld.lld'
        if not target.is_file() or wrapper.is_symlink():
            continue
        # A non-Nix toolchain may contain a binary here, rather than a script.
        with wrapper.open('rb') as source:
            if source.read(2) != b'#!':
                continue
        original = wrapper.read_text()
        updated = ABSOLUTE_LINKER.sub(lambda _: RELATIVE_LINKER, original)
        if updated != original:
            wrapper.write_text(updated)
            repaired += 1
            print(f'Repaired relocatable Rust linker: {wrapper.relative_to(project)}')
    return repaired


if __name__ == '__main__':
    repair(Path(__file__).resolve().parents[1])
