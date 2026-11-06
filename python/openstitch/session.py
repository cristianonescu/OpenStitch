"""The :class:`Session`: OpenStitch's command engine in a ``stitchtool`` child process."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import threading
from pathlib import Path
from typing import Any, Iterable, Mapping, Optional, Sequence, Union

PathLike = Union[str, "os.PathLike[str]"]


class OpenStitchError(RuntimeError):
    """A command the engine refused or that failed: ``command`` and the engine's ``message``."""

    def __init__(self, command: str, message: str):
        super().__init__(f"{command}: {message}")
        self.command = command
        self.message = message


class BatchError(OpenStitchError):
    """A :meth:`Session.batch` step that failed: ``step`` (0-based) and the ``results`` before it."""

    def __init__(self, step: int, command: str, message: str, results: list):
        super().__init__(command, message)
        self.step = step
        self.results = results

    def __str__(self) -> str:
        return f"step {self.step + 1} ({self.command}): {self.message}"


def find_stitchtool(program: Optional[PathLike] = None) -> str:
    """The ``stitchtool`` program: ``program``, else ``$OPENSTITCH_STITCHTOOL``, else the
    ``PATH``. Raises :class:`FileNotFoundError` when there is none."""
    for candidate in (program, os.environ.get("OPENSTITCH_STITCHTOOL")):
        if candidate:
            if Path(candidate).is_file():
                return str(candidate)
            raise FileNotFoundError(f"stitchtool not found at {candidate}")
    found = shutil.which("stitchtool")
    if not found:
        raise FileNotFoundError(
            "stitchtool is not on the PATH; install OpenStitch, or pass "
            "Session(stitchtool=...) or set OPENSTITCH_STITCHTOOL"
        )
    return found


class _Group:
    """``session.shape.ellipse(...)``: the commands of one group, by attribute."""

    def __init__(self, session: "Session", prefix: str):
        self._session = session
        self._prefix = prefix

    def __getattr__(self, name: str):
        if name.startswith("_"):
            raise AttributeError(name)
        command = f"{self._prefix}.{name}"

        def call(params: Optional[Mapping[str, Any]] = None, **kwargs: Any) -> Any:
            return self._session.exec(command, params, **kwargs)

        call.__name__ = name
        call.__doc__ = f"Run the engine command {command}."
        return call


class Session:
    """One OpenStitch design, edited by engine commands.

    ``Session()`` starts ``stitchtool serve`` with a new, empty design; relative file paths
    are resolved against ``cwd`` (default: the current directory). ``read_only`` refuses edits
    and file writes, ``no_files`` every file read and write, as ``stitchtool serve`` does.
    """

    def __init__(
        self,
        *,
        stitchtool: Optional[PathLike] = None,
        cwd: Optional[PathLike] = None,
        read_only: bool = False,
        no_files: bool = False,
        _args: Optional[Sequence[str]] = None,
    ):
        args = list(_args) if _args is not None else ["serve"]
        if _args is None and read_only:
            args.append("--read-only")
        if _args is None and no_files:
            args.append("--no-files")
        self._program = find_stitchtool(stitchtool)
        self._proc = subprocess.Popen(
            [self._program, *args],
            cwd=os.fspath(cwd) if cwd is not None else None,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            encoding="utf-8",
            bufsize=1,
        )
        self._lock = threading.Lock()
        self._next_id = 0
        self._commands: Optional[list] = None

    @classmethod
    def connect(
        cls, endpoint: Optional[PathLike] = None, *, stitchtool: Optional[PathLike] = None
    ) -> "Session":
        """Drive the running editor (started with ``openstitch --listen PORT``) through
        ``stitchtool connect``; ``endpoint`` is its endpoint file (default: automation.json in
        the OpenStitch config folder). The editor's own rules decide what is allowed."""
        args = ["connect"] + (["--endpoint", os.fspath(endpoint)] if endpoint else [])
        return cls(stitchtool=stitchtool, _args=args)

    # -- commands ---------------------------------------------------------------------------

    def request(self, command: str, params: Optional[Mapping[str, Any]] = None) -> dict:
        """Send one request and return the engine's raw reply (``{"ok", "result" | "error"}``)."""
        with self._lock:
            if self._proc.poll() is not None:
                raise OpenStitchError(command, self._exited())
            self._next_id += 1
            request: dict = {"id": self._next_id, "command": command}
            if params:
                request["params"] = dict(params)
            assert self._proc.stdin is not None and self._proc.stdout is not None
            try:
                self._proc.stdin.write(json.dumps(request) + "\n")
                self._proc.stdin.flush()
                line = self._proc.stdout.readline()
            except (BrokenPipeError, OSError) as e:
                raise OpenStitchError(command, f"stitchtool stopped ({e})") from e
            if not line:
                raise OpenStitchError(command, self._exited())
            return json.loads(line)

    def exec(
        self, command: str, params: Optional[Mapping[str, Any]] = None, **kwargs: Any
    ) -> Any:
        """Run ``command`` with ``params`` (a dict, keyword arguments, or both) and return its
        result. Raises :class:`OpenStitchError` with the engine's message when it fails.
        Parameter names are the engine's (``cornerRadii``, ``runTimes``); pass names that are
        Python keywords, such as ``from``, in the dict."""
        merged = {**(params or {}), **kwargs}
        reply = self.request(command, merged)
        if not reply.get("ok"):
            raise OpenStitchError(command, str(reply.get("error", "failed")))
        return reply.get("result")

    def batch(
        self, steps: Iterable[Union[Mapping[str, Any], Sequence[Any]]], *, continue_on_error: bool = False
    ) -> list:
        """Run several commands in order: each step is ``{"command", "params"}`` (as in a
        ``stitchtool exec`` script) or ``(command, params)``. Stops at the first failure with a
        :class:`BatchError` unless ``continue_on_error``, which puts the error message in that
        step's place in the returned list instead."""
        results: list = []
        for i, step in enumerate(steps):
            if isinstance(step, Mapping):
                command, params = step["command"], step.get("params")
            else:
                command, params = step[0], (step[1] if len(step) > 1 else None)
            reply = self.request(command, params)
            if reply.get("ok"):
                results.append(reply.get("result"))
            elif continue_on_error:
                results.append(OpenStitchError(command, str(reply.get("error"))))
            else:
                raise BatchError(i, command, str(reply.get("error", "failed")), results)
        return results

    def commands(self) -> list:
        """The engine's command catalogue (``commands.list``): every command with its help text
        and parameters."""
        if self._commands is None:
            result = self.exec("commands.list")
            self._commands = result if isinstance(result, list) else result.get("commands", [])
        return self._commands

    def describe(self, command: str) -> dict:
        """One command's entry from :meth:`commands`."""
        for entry in self.commands():
            if entry.get("id", entry.get("name")) == command:
                return entry
        raise OpenStitchError(command, "unknown command")

    # -- conveniences -----------------------------------------------------------------------

    def export(self, path: PathLike, **params: Any) -> Any:
        """``design.export``: write a machine file; the extension picks the format (.pes, .dst,
        .jef, ...)."""
        return self.exec("design.export", path=os.fspath(path), **params)

    def save(self, path: PathLike) -> Any:
        """``project.save``: write the editable project (.osd)."""
        return self.exec("project.save", path=os.fspath(path))

    def render(self, path: PathLike, **params: Any) -> Any:
        """``design.exportPicture``: a picture of the stitched design (.png, .bmp, ...)."""
        return self.exec("design.exportPicture", path=os.fspath(path), **params)

    def info(self) -> Any:
        """``design.info``: size, stitches, colours, trims and sewing time."""
        return self.exec("design.info")

    def objects(self) -> list:
        """``object.list``: the objects in sewing order."""
        return self.exec("object.list")

    def __getattr__(self, name: str) -> _Group:
        if name.startswith("_"):
            raise AttributeError(name)
        return _Group(self, name)

    # -- lifetime ---------------------------------------------------------------------------

    def _exited(self) -> str:
        err = self._proc.stderr.read().strip() if self._proc.stderr else ""
        return f"stitchtool exited ({self._proc.returncode}){': ' + err if err else ''}"

    def close(self) -> None:
        """End the session and its ``stitchtool`` process."""
        proc = self.__dict__.get("_proc")
        if proc is None or proc.poll() is not None:
            return
        try:
            if proc.stdin:
                proc.stdin.close()
            proc.wait(timeout=10)
        except (OSError, subprocess.TimeoutExpired):
            proc.kill()
            proc.wait()
        for stream in (proc.stdout, proc.stderr):
            if stream:
                stream.close()

    def __enter__(self) -> "Session":
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            self.close()
        except Exception:
            pass
