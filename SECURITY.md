# Security

Pro Wave (Yuri's Version) opens files from anywhere (audio, MIDI, projects, images) and runs
third-party plug-ins inside its own process, so a malicious file or plug-in can do harm. Open
projects and plug-ins only from sources you trust.

Help > Allow Control by Other Programs opens a Unix socket in the settings folder through which
any program running as you can drive the app: open and save projects, take screenshots of its
window, quit it. It is off until you choose it, readable and writable by your user only, and has
no network port. Turn it off when you do not use it.

To report a security problem, use **Report a vulnerability** on the Security tab of
[yuribeats/prowave](https://github.com/yuribeats/prowave/security), not a public issue. Say
which version, what file or plug-in triggers it, and what happens.
