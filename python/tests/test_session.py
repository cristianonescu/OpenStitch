"""Tests of the openstitch package against a real stitchtool (#498)."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

from openstitch import BatchError, OpenStitchError, Session, find_stitchtool  # noqa: E402


def stitchtool() -> str:
    exe = "stitchtool.exe" if os.name == "nt" else "stitchtool"
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    for build in ("debug", "release"):
        if (target / build / exe).is_file():
            return str(target / build / exe)
    return find_stitchtool()


try:
    TOOL = stitchtool()
except FileNotFoundError:
    TOOL = None

STEPS = [
    {"command": "design.new", "params": {"name": "Badge", "width": 100, "height": 100}},
    {"command": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 30, "fill": "#1a3d7c"}},
    {"command": "object.setLine", "params": {"id": 1, "type": "zigzag", "width": 3, "color": "Deep Gold"}},
    {"command": "shape.rectangle", "params": {"x": 35, "y": 45, "width": 30, "height": 10, "fill": "Red"}},
]


@unittest.skipIf(TOOL is None, "no stitchtool built")
class SessionTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory(prefix="openstitch-py-")
        self.tmp = Path(self.dir.name)

    def tearDown(self):
        self.dir.cleanup()

    def test_the_same_file_as_a_stitchtool_exec_script(self):
        # The PES label holds the file name, so both write badge.pes, in their own folders.
        (self.tmp / "exec").mkdir()
        (self.tmp / "py").mkdir()
        script = self.tmp / "exec" / "badge.json"
        steps = STEPS + [{"command": "design.export", "params": {"path": "badge.pes"}}]
        script.write_text(json.dumps(steps), encoding="utf-8")
        subprocess.run([TOOL, "exec", str(script)], check=True, capture_output=True)
        with Session(stitchtool=TOOL, cwd=self.tmp / "py") as s:
            s.exec("design.new", name="Badge", width=100, height=100)
            ring = s.exec("shape.ellipse", {"cx": 50, "cy": 50, "rx": 30, "fill": "#1a3d7c"})
            self.assertEqual(ring["id"], 1)
            # The attribute form is the same command.
            s.object.setLine(id=1, type="zigzag", width=3, color="Deep Gold")
            s.shape.rectangle(x=35, y=45, width=30, height=10, fill="Red")
            s.export("badge.pes")
            info = s.info()
            self.assertGreater(info["stitchStats"]["stitches"], 500)
            self.assertEqual(len(s.objects()), 2)
        produced = (self.tmp / "py" / "badge.pes").read_bytes()
        self.assertEqual(produced, (self.tmp / "exec" / "badge.pes").read_bytes())
        self.assertTrue(produced.startswith(b"#PES"))

    def test_errors_carry_the_command_and_the_engines_message(self):
        with Session(stitchtool=TOOL, cwd=self.tmp) as s:
            with self.assertRaises(OpenStitchError) as e:
                s.exec("shape.ellipse", cx=50, cy=50, rx=-4)
            self.assertEqual(e.exception.command, "shape.ellipse")
            self.assertIn("rx", e.exception.message)
            with self.assertRaises(OpenStitchError) as e:
                s.exec("shape.noSuchThing")
            self.assertIn("unknown", e.exception.message.lower())
            # The session goes on after an error.
            self.assertEqual(s.exec("shape.ellipse", cx=50, cy=50, rx=10)["id"], 1)

    def test_batch_stops_at_the_first_failure(self):
        with Session(stitchtool=TOOL, cwd=self.tmp) as s:
            results = s.batch(STEPS)
            self.assertEqual(len(results), 4)
            self.assertEqual(results[3]["id"], 2)
            with self.assertRaises(BatchError) as e:
                s.batch([("object.delete", {"ids": [2]}), ("object.move", {"ids": [99], "dx": 1, "dy": 0})])
            self.assertEqual(e.exception.step, 1)
            self.assertEqual(len(e.exception.results), 1)
            mixed = s.batch([("object.move", {"ids": [99], "dx": 1, "dy": 0}), ("design.info",)], continue_on_error=True)
            self.assertIsInstance(mixed[0], OpenStitchError)
            self.assertIn("stitchStats", mixed[1])

    def test_commands_render_and_save(self):
        with Session(stitchtool=TOOL, cwd=self.tmp) as s:
            entry = s.describe("shape.ellipse")
            self.assertIn("cx", json.dumps(entry))
            self.assertGreater(len(s.commands()), 100)
            s.batch(STEPS)
            s.render("badge.png")
            s.save("badge.osd")
        self.assertEqual((self.tmp / "badge.png").read_bytes()[:4], b"\x89PNG")
        self.assertTrue((self.tmp / "badge.osd").stat().st_size > 0)

    def test_read_only_refuses_file_writes(self):
        with Session(stitchtool=TOOL, cwd=self.tmp, read_only=True) as s:
            with self.assertRaises(OpenStitchError):
                s.exec("shape.ellipse", cx=50, cy=50, rx=10)
            self.assertIn("stitchStats", s.info())
        self.assertFalse((self.tmp / "x.pes").exists())

    def test_a_missing_program_is_reported(self):
        with self.assertRaises(FileNotFoundError):
            Session(stitchtool=self.tmp / "nothing-here.exe")


if __name__ == "__main__":
    unittest.main()
