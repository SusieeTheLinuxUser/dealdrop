#!/usr/bin/env python3
"""flatpak-spawn --host's D-Bus service, on run.sh's private bus: runs the command right here, inside the throwaway
session, so the real sandboxed app can ask the real `flatpak` about a throwaway install. Prints "ready" when up."""
import os
import subprocess

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

IFACE = "org.freedesktop.Flatpak.Development"
PATH = "/org/freedesktop/Flatpak/Development"
XML = f"""<node><interface name="{IFACE}">
  <method name="HostCommand"><arg type="ay" direction="in"/><arg type="aay" direction="in"/><arg type="a{{uh}}" direction="in"/>
    <arg type="a{{ss}}" direction="in"/><arg type="u" direction="in"/><arg type="u" direction="out"/></method>
  <signal name="HostCommandExited"><arg type="u"/><arg type="u"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""


def text(ay):
    return bytes(ay).rstrip(b"\0").decode()


def call(conn, sender, path, iface, method, params, invocation):
    cwd, argv, fds, env, _flags = params.unpack()
    fd_list = invocation.get_message().get_unix_fd_list()
    got = {n: fd_list.get(i) for n, i in fds.items()}
    proc = subprocess.Popen([text(a) for a in argv], cwd=text(cwd) or None, env={**os.environ, **env},
                            stdin=got.get(0), stdout=got.get(1), stderr=got.get(2))
    for fd in got.values():
        os.close(fd)
    GLib.child_watch_add(GLib.PRIORITY_DEFAULT, proc.pid, lambda pid, status: conn.emit_signal(
        sender, PATH, IFACE, "HostCommandExited", GLib.Variant("(uu)", (pid, status))))
    invocation.return_value(GLib.Variant("(u)", (proc.pid,)))


def main():
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.register_object(PATH, info, call, lambda *_: GLib.Variant("u", 1), None)
    Gio.bus_own_name_on_connection(bus, "org.freedesktop.Flatpak", 0, lambda *_: print("ready", flush=True), None)
    GLib.MainLoop().run()


main()
