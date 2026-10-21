#!/usr/bin/env python3
"""The two desktop services DealDrop talks to, on run.sh's private bus: a StatusNotifierWatcher (what
Plasma's panel provides for tray icons) and a notification server. Prints one JSON line per event:
{"tray": {"sender": …, "service": …}} and {"notify": {"app": …, "summary": …, "body": …}}."""
import json
import sys

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402

WATCHER = """<node><interface name="org.kde.StatusNotifierWatcher">
  <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
  <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
  <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
  <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
  <property name="ProtocolVersion" type="i" access="read"/>
  <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
  <signal name="StatusNotifierHostRegistered"/>
</interface></node>"""
NOTIFY = """<node><interface name="org.freedesktop.Notifications">
  <method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/>
    <arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/>
    <arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
  <method name="GetCapabilities"><arg type="as" direction="out"/></method>
  <method name="GetServerInformation"><arg type="s" direction="out"/><arg type="s" direction="out"/>
    <arg type="s" direction="out"/><arg type="s" direction="out"/></method>
  <method name="CloseNotification"><arg type="u" direction="in"/></method>
  <signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
  <signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>"""

items = []
next_id = [0]


def emit(event):
    print(json.dumps(event), flush=True)


def watcher_call(conn, sender, path, iface, method, params, invocation):
    if method == "RegisterStatusNotifierItem":
        service = params.unpack()[0]
        items.append(service if service.startswith(":") else f"{sender}{service}")
        emit({"tray": {"sender": sender, "service": service}})
    invocation.return_value(None)


def watcher_prop(conn, sender, path, iface, name):
    return {"RegisteredStatusNotifierItems": GLib.Variant("as", items),
            "IsStatusNotifierHostRegistered": GLib.Variant("b", True),
            "ProtocolVersion": GLib.Variant("i", 0)}[name]


def notify_call(conn, sender, path, iface, method, params, invocation):
    if method == "Notify":
        app, _replaces, _icon, summary, body, *_ = params.unpack()
        next_id[0] += 1
        emit({"notify": {"app": app, "summary": summary, "body": body}})
        invocation.return_value(GLib.Variant("(u)", (next_id[0],)))
    elif method == "GetCapabilities":
        invocation.return_value(GLib.Variant("(as)", (["body"],)))
    elif method == "GetServerInformation":
        invocation.return_value(GLib.Variant("(ssss)", ("e2e", "dealdrop", "1", "1.2")))
    else:
        invocation.return_value(None)


def main():
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    for xml, path, call, prop in [(WATCHER, "/StatusNotifierWatcher", watcher_call, watcher_prop),
                                  (NOTIFY, "/org/freedesktop/Notifications", notify_call, None)]:
        iface = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]
        bus.register_object(path, iface, call, prop, None)
    for name in ["org.kde.StatusNotifierWatcher", "org.freedesktop.Notifications"]:
        reply = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName",
                              GLib.Variant("(su)", (name, 4)), None, Gio.DBusCallFlags.NONE, -1, None)
        if reply.unpack()[0] != 1:
            sys.exit(f"couldn't own {name}")
    emit({"ready": True})
    GLib.MainLoop().run()


main()
