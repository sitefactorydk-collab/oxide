//! POSIX `sh` scripts run inside sandbox containers. Arguments follow the script (`sh -c
//! SCRIPT oxide ARG1 ARG2`), so nothing a model writes is ever pasted into a script.
//! They only use what Debian, Ubuntu and Alpine (busybox) images all have.

/// The container's main process: waits until Docker stops it.
pub const KEEP_ALIVE: &str = "trap 'exit 0' TERM INT; while :; do sleep 86400 & wait $!; done";

/// Runs `$1` in folder `$2`, in bash when the image has it, else sh.
pub const EXEC: &str = r#"cd -- "$2" 2>/dev/null || { printf '%s\n' "The folder $2 doesn't exist in the sandbox." >&2; exit 2; }
if command -v bash >/dev/null 2>&1; then exec bash -c "$1"; fi
exec sh -c "$1""#;

/// Kills every process of run `$1` (they carry `OXIDE_EXEC=$1` in their environment).
pub const KILL_RUN: &str = r#"for p in /proc/[0-9]*; do
  if tr '\0' '\n' < "$p/environ" 2>/dev/null | grep -qx "OXIDE_EXEC=$1"; then kill -9 "${p#/proc/}" 2>/dev/null; fi
done
exit 0"#;

/// Lists folder `$1` (at most `$2` entries): `kind<TAB>size<TAB>name` per line, `+` when
/// cut off. Exits 3 when `$1` isn't a folder.
pub const LIST: &str = r#"cd -- "$1" 2>/dev/null || exit 3
n=0
for f in * .[!.]* ..?*; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  n=$((n+1))
  if [ "$n" -gt "$2" ]; then echo +; break; fi
  if [ -L "$f" ]; then printf 'l\t\t%s\n' "$f"
  elif [ -d "$f" ]; then printf 'd\t\t%s\n' "$f"
  else s=$(stat -c %s -- "$f" 2>/dev/null || wc -c < "$f" | tr -d ' '); printf 'f\t%s\t%s\n' "$s" "$f"; fi
done"#;

/// Checks `$1` before it is copied out: exit 3 when it doesn't exist, 4 when it is or holds
/// a symbolic link; otherwise prints its size in KB.
pub const CHECK_OUT: &str = r#"[ -e "$1" ] || [ -L "$1" ] || exit 3
if [ -n "$(find "$1" -type l 2>/dev/null | head -n 1)" ]; then exit 4; fi
du -sk -- "$1" 2>/dev/null | cut -f1"#;
