# openstitch (Python)

Script OpenStitch embroidery designs from Python (#498): build, edit, digitize and export
designs from a script or a notebook with the same commands as the editor, `stitchtool exec`
scripts and the AI Assistant.

The package is pure Python with no dependencies. It runs the engine in a `stitchtool serve`
child process, so it needs OpenStitch's `stitchtool` program: on the `PATH`, in
`$OPENSTITCH_STITCHTOOL`, or passed as `Session(stitchtool="...")`. It is not on PyPI yet;
install it from this folder:

```bash
pip install ./python
```

## Example

```python
from openstitch import Session, OpenStitchError

with Session() as s:
    s.exec("design.new", name="Badge", width=100, height=100)
    badge = s.exec("shape.ellipse", cx=50, cy=50, rx=40, fill="#1a3d7c")["id"]
    # Groups by attribute: s.object.setLine(...) is s.exec("object.setLine", ...).
    s.object.setRegion(id=badge, type="fill", angle=45)
    s.object.setLine(id=badge, type="zigzag", width=3, color="Deep Gold")
    s.text.add(text="OPEN", size=12, x=50, y=56, align="center", fill="White")
    print(s.info()["stitchStats"])
    s.export("badge.pes")      # the extension picks the format
    s.render("badge.png")      # a picture of the stitches
```

- `Session.exec(command, params=None, **kwargs)` runs one command and returns its result.
  Parameter names are the engine's (`cornerRadii`, `runTimes`); pass a Python keyword such as
  `from` in the `params` dict.
- A failed command raises `OpenStitchError` with `.command` and the engine's `.message`.
- `Session.batch([...])` runs `{"command", "params"}` steps (a `stitchtool exec` script's
  steps) or `(command, params)` pairs, stopping at the first failure with `BatchError`
  (`.step`, `.results`), or with `continue_on_error=True`.
- `Session.commands()` lists every command with its help text and parameters;
  `Session.describe("shape.ellipse")` gives one.
- `export`, `save` (`.osd` project), `render`, `info` and `objects` are short forms of
  `design.export`, `project.save`, `design.exportPicture`, `design.info` and `object.list`.
- `Session(read_only=True)` refuses edits and file writes; `Session(no_files=True)` refuses
  every file read and write. File paths are resolved against `cwd`.
- `Session.connect()` drives the running editor (started with `openstitch --listen PORT`)
  instead of a design of its own; the editor's own rules decide what is allowed.

## Tests

```bash
python -m unittest discover -s python/tests
```

The tests need a built `stitchtool` (`OPENSTITCH_STITCHTOOL`, or `target/debug` /
`target/release` of this repository, or the `PATH`).
