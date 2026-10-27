"""Regression checks for moving a Devbox checkout after installing Rust."""

from pathlib import Path
import subprocess
import tempfile
import unittest

from repair_rustup_linker import repair


class RustupLinkerTests(unittest.TestCase):
    def test_wrapper_links_after_checkout_move_and_preserves_arguments(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary) / 'original checkout'
            linker_bin = project / '.devbox/virtenv/rustup/toolchains/test/lib/rustlib/host/bin'
            wrapper = linker_bin / 'gcc-ld/ld.lld'
            target = linker_bin / 'gcc-ld-unwrapped/ld.lld'
            wrapper.parent.mkdir(parents=True)
            target.parent.mkdir()
            target.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
            target.chmod(0o755)
            wrapper.write_text(
                '#!/usr/bin/env bash\nset -eu\n'
                f'export PROG="{target}"\n'
                'exec "$PROG" "$@"\n'
            )
            wrapper.chmod(0o755)
            self.assertEqual(repair(project), 1)
            moved = Path(temporary) / 'moved checkout'
            project.rename(moved)
            moved_wrapper = moved / wrapper.relative_to(project)
            result = subprocess.run(
                [str(moved_wrapper), 'object with spaces.o', '-o', 'output'],
                check=True, capture_output=True, text=True,
            )
            self.assertEqual(result.stdout.splitlines(), ['object with spaces.o', '-o', 'output'])
            self.assertEqual(repair(moved), 0)

    def test_missing_toolchains_are_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assertEqual(repair(Path(temporary)), 0)


if __name__ == '__main__':
    unittest.main()
