"""Oxide's launcher for the run_python tool.

Runs the script it is given as __main__, in the current folder, and afterwards saves
matplotlib figures the script left open (as figure-N.png) unless it wrote an image itself.
Tracebacks start at the script's own frame.
"""

import os
import runpy
import sys
import traceback
import warnings

IMAGE_EXTENSIONS = (".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg")


def image_state(folder):
    state = {}
    try:
        for entry in os.scandir(folder):
            if entry.is_file() and entry.name.lower().endswith(IMAGE_EXTENSIONS):
                state[entry.name] = entry.stat().st_mtime_ns
    except OSError:
        pass
    return state


def save_open_figures(folder, before):
    pyplot = sys.modules.get("matplotlib.pyplot")
    if pyplot is None:
        return
    try:
        numbers = pyplot.get_fignums()
        if not numbers or image_state(folder) != before:
            return
        count = 0
        for number in numbers:
            count += 1
            name = "figure-%d.png" % count
            while os.path.exists(os.path.join(folder, name)):
                count += 1
                name = "figure-%d.png" % count
            pyplot.figure(number).savefig(
                os.path.join(folder, name), dpi=150, bbox_inches="tight"
            )
        pyplot.close("all")
    except Exception:
        traceback.print_exc()


def main():
    script = sys.argv[1]
    folder = os.getcwd()
    sys.argv = [script]
    sys.path.insert(0, folder)
    warnings.filterwarnings("ignore", message=".*non-interactive.*")
    before = image_state(folder)
    code = 0
    try:
        runpy.run_path(script, run_name="__main__")
    except SystemExit as exit_:
        value = exit_.code
        if value is None:
            code = 0
        elif isinstance(value, int):
            code = value
        else:
            print(value, file=sys.stderr)
            code = 1
    except BaseException as error:
        tb = error.__traceback__
        while tb is not None and tb.tb_frame.f_code.co_filename != script:
            tb = tb.tb_next
        traceback.print_exception(type(error), error, tb or error.__traceback__)
        code = 1
    finally:
        save_open_figures(folder, before)
    sys.stdout.flush()
    sys.stderr.flush()
    sys.exit(code)


main()
