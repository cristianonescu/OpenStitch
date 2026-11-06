"""Script OpenStitch embroidery designs from Python (#498).

    from openstitch import Session

    with Session() as s:
        s.exec("design.new", name="Badge", width=100, height=100)
        badge = s.exec("shape.ellipse", cx=50, cy=50, rx=40, fill="#1a3d7c")["id"]
        s.object.setLine(id=badge, type="zigzag", width=3, color="Deep Gold")
        s.export("badge.pes")

A :class:`Session` runs the OpenStitch engine in a ``stitchtool serve`` child process and
sends it the same commands the editor, ``stitchtool exec`` scripts and the AI Assistant use;
``stitchtool commands`` (or :meth:`Session.commands`) lists them with their parameters.
:meth:`Session.connect` drives a running editor instead (``openstitch --listen PORT``).
The package is pure Python with no dependencies; it needs a ``stitchtool`` program.
"""

from .session import BatchError, OpenStitchError, Session, find_stitchtool

__all__ = ["BatchError", "OpenStitchError", "Session", "find_stitchtool"]
__version__ = "0.1.0a13"
