#!/usr/bin/env python3
"""Thin KiCad IPC client for the local RateMyPCB executable."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import wx
from kipy import KiCad
from kipy.errors import ConnectionError as KiCadConnectionError


def executable():
    override = os.environ.get("RATEMYPCB_BINARY", "").strip()
    if override:
        path = Path(override).expanduser()
        if path.is_file():
            return str(path)
    sibling = Path(__file__).resolve().parent / "bin" / (
        "ratemypcb.exe" if sys.platform == "win32" else "ratemypcb"
    )
    if sibling.is_file():
        return str(sibling)
    return shutil.which("ratemypcb")


def active_board_path():
    board = KiCad(client_name="RateMyPCB").get_board()
    project_path = Path(board.document.project.path)
    filename = Path(board.document.board_filename)
    path = filename if filename.is_absolute() else project_path / filename
    if not path.is_file():
        raise RuntimeError("Save the active board before running RateMyPCB.")
    return path.resolve()


def render(report):
    lines = [
        f"{report['score']['value']:.1f}/10 — {report['score']['verdict']}",
        f"Confidence: {report['confidence']}",
        "",
    ]
    findings = report.get("findings", [])
    if not findings:
        lines.append("No deterministic findings in the checks that ran.")
    for item in findings[:40]:
        lines.extend(
            [
                f"[{item['severity'].upper()}] {item['title']}",
                item["evidence"],
                f"Fix: {item['recommendation']}",
                "",
            ]
        )
    if len(findings) > 40:
        lines.append(f"{len(findings) - 40} more findings are available in CLI JSON output.\n")
    lines.append(report["disclaimer"])
    return "\n".join(lines)


def main():
    app = wx.App.Get() or wx.App(False)
    try:
        binary = executable()
        if not binary:
            raise RuntimeError(
                "RateMyPCB is not installed. Install a standalone release or set "
                "RATEMYPCB_BINARY to its full path."
            )
        board = active_board_path()
        process = subprocess.run(
            [binary, "review", str(board), "--format", "json", "--native", "off"],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        if process.returncode not in (0, 1):
            raise RuntimeError(process.stderr.strip() or f"RateMyPCB exited {process.returncode}.")
        report = json.loads(process.stdout)
        dialog = wx.Dialog(None, title="RateMyPCB DFM Review", size=(780, 620))
        panel = wx.Panel(dialog)
        text = wx.TextCtrl(panel, value=render(report), style=wx.TE_MULTILINE | wx.TE_READONLY | wx.TE_RICH2)
        close = wx.Button(panel, wx.ID_OK, "Close")
        layout = wx.BoxSizer(wx.VERTICAL)
        layout.Add(text, 1, wx.EXPAND | wx.ALL, 12)
        layout.Add(close, 0, wx.ALIGN_RIGHT | wx.LEFT | wx.RIGHT | wx.BOTTOM, 12)
        panel.SetSizer(layout)
        dialog.ShowModal()
        dialog.Destroy()
    except (RuntimeError, OSError, ValueError, subprocess.TimeoutExpired, KiCadConnectionError) as error:
        wx.MessageBox(str(error), "RateMyPCB", wx.OK | wx.ICON_ERROR)


if __name__ == "__main__":
    main()

