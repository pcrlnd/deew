#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import ntpath
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from base64 import b64decode
from builtins import print as oprint
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
from datetime import timedelta
from glob import glob
from multiprocessing import cpu_count
from types import SimpleNamespace
from typing import Any, NoReturn

import requests
import toml
import xmltodict
from packaging import version
from platformdirs import PlatformDirs
from rich import print
from rich.console import Console
from rich.progress import BarColumn, Progress, TaskID
from rich.prompt import Confirm, Prompt
from rich.syntax import Syntax
from rich.table import Table
from unidecode import unidecode

sys.path.append('.')

from deew.bitrates import allowed_bitrates
from deew.logos import logos
from deew.messages import error_messages
from deew.xml_base import xml_ac4_base, xml_dd_ddp_base, xml_thd_base

prog_name = 'deew'
prog_version = '3.2.2'

simplens = SimpleNamespace()


class RParse(argparse.ArgumentParser):
    def _print_message(self, message, file=None):
        if message:
            if message.startswith('usage'):
                message = f'[bold cyan]{prog_name}[/bold cyan] {prog_version}\n\n{message}'
                message = re.sub(r'(-[a-z]+\s*|\[)([A-Z]+)(?=]|,|\s\s|\s\.)', r'\1[{}]\2[/{}]'.format('bold color(231)', 'bold color(231)'), message)
                message = re.sub(r'((-|--)[a-z]+)', r'[{}]\1[/{}]'.format('green', 'green'), message)
                message = message.replace('usage', f'[yellow]USAGE[/yellow]')
                message = message.replace('options', f'[yellow]FLAGS[/yellow]', 1)
                message = message.replace(self.prog, f'[bold cyan]{self.prog}[/bold cyan]')
            message = f'[not bold white]{message.strip()}[/not bold white]'
            print(message)


class CustomHelpFormatter(argparse.RawTextHelpFormatter):
    def _format_action_invocation(self, action):
        if not action.option_strings or action.nargs == 0:
            return super()._format_action_invocation(action)
        default = self._get_default_metavar_for_optional(action)
        args_string = self._format_args(action, default)
        return ', '.join(action.option_strings) + ' ' + args_string


parser = RParse(
    prog=prog_name,
    add_help=False,
    formatter_class=lambda prog: CustomHelpFormatter(prog, width=78, max_help_position=32)
)
parser.add_argument('-h', '--help',
                    action='help',
                    default=argparse.SUPPRESS,
                    help='show this help message.')
parser.add_argument('-v', '--version',
                    action='version',
                    version=f'[bold cyan]{prog_name}[/bold cyan] [not bold white]{prog_version}[/not bold white]',
                    help='show version.')
parser.add_argument('-i', '--input',
                    nargs='*',
                    default=argparse.SUPPRESS,
                    help='audio file(s) or folder(s)')
parser.add_argument('-ti', '--track-index',
                    type=int,
                    default=0,
                    metavar='INDEX',
                    help=
'''[underline magenta]default:[/underline magenta] [bold color(231)]0[/bold color(231)]
select audio track index of input(s)''')
parser.add_argument('-o', '--output',
                    default=None,
                    metavar='DIRECTORY',
                    help='[underline magenta]default:[/underline magenta] current directory\nspecifies output directory')
parser.add_argument('-f', '--format',
                    type=str,
                    default='ddp',
                    help=
'''[underline magenta]options:[/underline magenta] [bold color(231)]dd[/bold color(231)] / [bold color(231)]ddp[/bold color(231)] / [bold color(231)]ac4[/bold color(231)] / [bold color(231)]thd[/bold color(231)]
[underline magenta]default:[/underline magenta] [bold color(231)]ddp[/bold color(231)]''')
parser.add_argument('-b', '--bitrate',
                    type=int,
                    default=None,
                    help=
'''[underline magenta]options:[/underline magenta] run [green]-lb[/green]/[green]--list-bitrates[/green]
[underline magenta]default:[/underline magenta] run [green]-c[/green]/[green]--config[/green]''')
parser.add_argument('-dm', '--downmix',
                    type=int,
                    default=None,
                    metavar='CHANNELS',
                    help=
'''[underline magenta]options:[/underline magenta] [bold color(231)]1[/bold color(231)] / [bold color(231)]2[/bold color(231)] / [bold color(231)]6[/bold color(231)]
specifies downmix, only works for DD/DDP
DD will be automatically downmixed to 5.1 in case of a 7.1 source''')
parser.add_argument('-d', '--delay',
                    type=str,
                    default=None,
                    help=
'''[underline magenta]examples:[/underline magenta] [bold color(231)]-5.1ms[/bold color(231)], [bold color(231)]+1,52s[/bold color(231)], \
[bold color(231)]-24@pal[/bold color(231)], [bold color(231)]+10@24000/1001[/bold color(231)]
[underline magenta]default:[/underline magenta] [bold color(231)]0ms[/bold color(231)] or parsed from filename
specifies delay as ms, s or frame@FPS
FPS can be a number, division or ntsc / pal
you have to specify negative values as [bold color(231)]-[/bold color(231)][bold color(231)]d=-0ms[/bold color(231)]''')
parser.add_argument('-r', '--drc',
                    type=str,
                    default='music_light',
                    help=
'''[underline magenta]options:[/underline magenta] [bold color(231)]film_light[/bold color(231)] / [bold color(231)]film_standard[/bold color(231)] / \
[bold color(231)]music_light[/bold color(231)] / [bold color(231)]music_standard[/bold color(231)] / [bold color(231)]speech[/bold color(231)]
[underline magenta]default:[/underline magenta] [bold color(231)]music_light[/bold color(231)] (this is the closest to the missing none preset)
specifies drc profile''')
parser.add_argument('-dn', '--dialnorm',
                    type=int,
                    default=0,
                    help=
'''[underline magenta]options:[/underline magenta] between [bold color(231)]-31[/bold color(231)] and [bold color(231)]0[/bold color(231)] \
(in case of [bold color(231)]0[/bold color(231)] DEE\'s measurement will be used)
[underline magenta]default:[/underline magenta] [bold color(231)]0[/bold color(231)]
applied dialnorm value between''')
parser.add_argument('-in', '--instances',
                    type=str,
                    default=None,
                    help=
'''[underline magenta]examples:[/underline magenta] [bold color(231)]1[/bold color(231)], [bold color(231)]4[/bold color(231)], [bold color(231)]50%%[/bold color(231)]
[underline magenta]default:[/underline magenta] [bold color(231)]50%%[/bold color(231)]
specifies how many encodes can run at the same time
[bold color(231)]50%%[/bold color(231)] means [bold color(231)]4[/bold color(231)] on a cpu with 8 threads
one DEE can use 2 threads so [bold color(231)]50%%[/bold color(231)] can utilize all threads
(this option overrides the config\'s number)''')
parser.add_argument('-k', '--keeptemp',
                    action='store_true',
                    help='keep temp files')
parser.add_argument('-mo', '--measure-only',
                    action='store_true',
                    help='kills DEE when the dialnorm gets written to the progress bar\nthis option overrides format with ddp')
parser.add_argument('-fs', '--force-standard',
                    action='store_true',
                    help='force standard profile for 7.1 DDP encoding (384-1024 kbps)')
parser.add_argument('-fb', '--force-bluray',
                    action='store_true',
                    help='force bluray profile for 7.1 DDP encoding (768-1664 kbps)')
parser.add_argument('-lb', '--list-bitrates',
                    action='store_true',
                    help='list bitrates that DEE can do for DD and DDP encoding')
parser.add_argument('-la', '--long-argument',
                    action='store_true',
                    help='print ffmpeg and DEE arguments for each input')
parser.add_argument('-np', '--no-prompt',
                    action='store_true',
                    help='disables prompt')
parser.add_argument('-pl', '--print-logos',
                    action='store_true',
                    help='show all logo variants you can set in the config')
parser.add_argument('-cl', '--changelog',
                    action='store_true',
                    help='show changelog')
parser.add_argument('-c', '--config',
                    action='store_true',
                    help='show config and config location(s)')
parser.add_argument('-gc', '--generate-config',
                    action='store_true',
                    help='generate a new config')
args = parser.parse_args()


def print_changelog() -> None:
    try:
        r = requests.get('https://api.github.com/repos/pcroland/deew/contents/changelog.md')
        changelog = json.loads(r.text)['content']
        changelog = b64decode(changelog).decode().split('\n\n')
        changelog.reverse()
        changelog = '\n\n'.join(changelog[-10:])
        changelog = changelog.split('\n')
    except Exception:
        print_exit('changelog')

    for line in changelog:
        if line.endswith('\\'): line = line[:-1]
        line = line.replace('\\', '\\\\')
        if line.startswith('# '): line = f'[bold color(231)]{line.replace("# ", "")}[/bold color(231)]'
        code_number = line.count('`')
        state_even = False
        for _ in range(code_number):
            if not state_even:
                line = line.replace('`', '[bold yellow]', 1)
                state_even = True
            else:
                line = line.replace('`', '[/bold yellow]', 1)
                state_even = False
        print(f'[not bold white]{line}[/not bold white]')
    sys.exit(0)


def print_logos() -> None:
    for i, logo in enumerate(logos):
        print(f'logo {i + 1}:\n{logo}')
    sys.exit(0)


def list_bitrates() -> None:
    for codec, bitrates in allowed_bitrates.items():
        print(f'[bold magenta]{codec}[/bold magenta]: [not bold color(231)]{"[white], [/white]".join([str(int) for int in bitrates])}[/not bold color(231)]')
    sys.exit(0)


def generate_config(standalone: bool, conf1: str, conf2: str, conf_dir: str) -> None:
    config_content = '''# These are required.
# If only name is specified, it will look in your system PATH variable, which includes the current directory on Windows.
# Setup instructions: https://github.com/pcroland/deew#setup-system-path-variable
# If full path is specified, that will be used.
ffmpeg_path = 'ffmpeg'
ffprobe_path = 'ffprobe'
dee_path = 'dee.exe'

# If this is empty, the default OS temporary directory will be used (or `temp` next to the script if you use the exe).
# You can also specify an absolute path or a path relative to the current directory.
temp_path = ''

# Set between 1 and 10, use the -pl/--print-logos option to see the available logos, set to 0 to disable logo.
logo = 1

# Specifies how many encodes can run at the same time.
# It can be a number or a % compared to your number of threads (so '50%' means 4 on an 8 thread cpu).
# One DEE can use 2 threads so setting '50%' can utilize all threads.
# You can override this setting with -in/--instances.
# The number will be clamped between 1 and cpu_count().
# With the Windows version of DEE the max will be cpu_count() - 2 or 6 due to a limitation.
# examples: 1, 4, '50%'
max_instances = '50%'

[default_bitrates]
    dd_1_0 = 128
    dd_2_0 = 256
    dd_5_1 = 640
    ddp_1_0 = 128
    ddp_2_0 = 256
    ddp_5_1 = 1024
    ddp_7_1 = 1536
    ac4_2_0 = 320

# You can toggle what sections you would like to see in the encoding summary
[summary_sections]
    deew_info = true
    binaries = true
    input_info = true
    output_info = true
    other = true
'''

    if standalone:
        print(f'''Please choose config's location:
[bold magenta]1[/bold magenta]: {conf1}
[bold magenta]2[/bold magenta]: {conf2}''')
        c_loc = Prompt.ask('Location', choices=['1','2'])
        if c_loc == '1':
            createdir(conf_dir)
            c_loc = conf1
        else:
            c_loc = conf2
    else:
        c_loc = conf1
        createdir(conf_dir)

    with open(c_loc, 'w') as fl:
        fl.write(config_content)
    print()
    Console().print(Syntax(config_content, 'toml'))
    print(f'\n[bold cyan]The above config has been created at:[/bold cyan]\n{c_loc}')
    sys.exit(1)


def clamp(inp: int, low: int, high: int) -> int:
    return min(max(inp, low), high)


def trim_names(fl: str, compensate: int) -> str:
    if len(fl) > 40 - compensate:
        fl = f'{fl[0:37 - compensate]}...'
    return fl.ljust(40 - compensate, ' ')


def stamp_to_sec(stamp):
    l = stamp.split(':')
    return int(l[0])*3600 + int(l[1])*60 + float(l[2])


def parse_version_string(inp: list) -> str:
    try:
        v = subprocess.run(inp, capture_output=True, encoding='utf-8').stdout
        v = v.split('\n')[0].split(' ')[2]
        v = v.replace(',', '').replace('-static', '')
        if len(v) > 30:
            v = f'{v[0:27]}...'
    except Exception:
        v = "[red]couldn't parse"
    return v


def convert_delay_to_ms(inp, compensate):
    if not inp.startswith(('-', '+')): print_exit('delay')
    inp = inp.replace(',', '.')

    negative = inp.startswith(('-'))

    if '@' in inp:
        frame = round(float(re.sub('[^0-9\.]', '', inp.split('@')[0])))
        if not frame: print_exit('delay')

        fps = str(inp.split('@')[1])
        if fps == 'ntsc': fps = str(24000 / 1001)
        if fps == 'pal': fps = str(25)
        if '/' in fps:
            divident = re.sub('[^0-9\.]', '', fps.split('/')[0])
            divisor = re.sub('[^0-9\.]', '', fps.split('/')[1])
            if not divident or not divisor: print_exit('delay')
            fps = float(divident) / float(divisor)
        delay = frame / float(fps) * 1000
    else:
        if not inp.endswith(('s', 'ms')): print_exit('delay')
        if not inp.count('.') < 2: print_exit('delay')

        if inp.endswith('ms'):
            delay = float(re.sub('[^0-9\.]', '', inp))
        else:
            delay = float(re.sub('[^0-9\.]', '', inp)) * 1000

    delay = float(f'-{delay}' if negative else f'+{delay}')
    delay_print = f'{format(delay, ".3f")} ms'

    if compensate: delay -= 16 / 3 # 256 / 48000 * 1000

    delay_mode = 'prepend_silence_duration'
    if delay < 0:
        delay_mode = 'start'
        delay_xml = str(timedelta(seconds = (abs(delay) / 1000)))
        if '.' not in delay_xml:
            delay_xml = f'{delay_xml}.0'
    else:
        delay_xml = format(delay / 1000, ".6f")

    return delay_print, delay_xml, delay_mode


def channel_number_to_name(inp: int):
    channel_names = {
        1: 'mono',
        2: 'stereo',
        6: '5.1',
        8: '7.1'
    }
    return channel_names[inp]


def find_closest_allowed(value: int, allowed_values: list[int]) -> int:
    return min(allowed_values, key=lambda list_value: abs(list_value - value))


def wpc(p: str, quote: bool=False) -> str:
    if not simplens.is_nonnative_exe:
        if quote: p = f'\"{p}\"'
        return p
    if not p.startswith('/mnt/'): print_exit('wsl_path', p)
    parts = list(filter(None, p.split('/')))[1:]
    parts[0] = parts[0].upper() + ':'
    p = '\\'.join(parts) + '\\'
    if quote: p = f'\"{p}\"'
    return p


def rwpc(p: str) -> str:
    return re.sub(r'^([a-z]):/', lambda m: f'/mnt/{m.group(1).lower()}/', p.replace('\\', '/'), flags=re.IGNORECASE)


def save_xml(f: str, xml: dict[str, Any]) -> None:
    with open(f, 'w', encoding='utf-8') as fd:
        fd.write(xmltodict.unparse(xml, pretty=True, indent='  ').replace('&amp;', '&'))


def basename(fl: str, format_: str, quote: bool=False, sanitize: bool=False, stripdelay: bool=False) -> str:
    name = os.path.basename(os.path.splitext(fl)[0]) + f'.{format_}'
    if stripdelay: name = re.sub(r' ?DELAY [-|+]?[0-9]+m?s', '', name)
    if sanitize: name = unidecode(name).replace(' ', '_')
    if quote: name = f'\"{name}\"'
    return name


def print_exit(message: str, insert: Any = None) -> NoReturn:
    if insert and '🤠' in error_messages[message]:
        message_split = error_messages[message].split('🤠')
        before, after = message_split[0], message_split[1]
        exit_message = f'[color(231) on red]ERROR:[/color(231) on red] {before}{insert}{after}'
    else:
        exit_message = f'[color(231) on red]ERROR:[/color(231) on red] {error_messages[message]}'
    print(exit_message)
    sys.exit(1)


def createdir(out: str) -> None:
    try:
        os.makedirs(out, exist_ok=True)
    except OSError:
        print_exit('create_dir', out)


def encode(task_id: TaskID, settings: list) -> None:
    config, pb = simplens.config, simplens.pb
    fl, output, length, ffmpeg_args, dee_args, intermediate_exists, aformat = settings
    fl_b = os.path.basename(fl)
    pb.update(description=f'[bold][cyan]starting[/cyan][/bold]...{" " * 24}', task_id=task_id, visible=True)

    if not intermediate_exists:
        if length == -1:
            pb.update(description=f'[bold][cyan]ffmpeg[/cyan][/bold] | {trim_names(fl_b, 6)}', task_id=task_id, total=None)
            ffmpeg = subprocess.run(ffmpeg_args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, universal_newlines=True, encoding='utf-8', errors='ignore')
        else:
            pb.update(description=f'[bold][cyan]ffmpeg[/cyan][/bold] | {trim_names(fl_b, 6)}', task_id=task_id, total=100)
            ffmpeg = subprocess.Popen(ffmpeg_args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, universal_newlines=True, encoding='utf-8', errors='ignore')
            percentage_length = length / 100
            with ffmpeg.stdout:
                for line in iter(ffmpeg.stdout.readline, ''):
                    if '=' not in line: continue
                    progress = re.search(r'time=([^\s]+)', line)
                    if progress:
                        timecode = stamp_to_sec(progress[1]) / percentage_length
                        pb.update(task_id=task_id, completed=timecode)
        pb.update(task_id=task_id, completed=100)
        time.sleep(0.5)

    if args.dialnorm != 0 and aformat == 'thd':
        pb.update(description=f'[bold cyan]DEE[/bold cyan]: encode | {trim_names(fl_b, 11)}', task_id=task_id, completed=0, total=100)
    else:
        pb.update(description=f'[bold cyan]DEE[/bold cyan]: measure | {trim_names(fl_b, 12)}', task_id=task_id, completed=0, total=100)
    dee = subprocess.Popen(dee_args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True, encoding='utf-8', errors='ignore')
    with dee.stdout:
        encoding_step = False
        for line in iter(dee.stdout.readline, ''):
            if not encoding_step and re.search(r'(measured_loudness|speech gated loudness)', line):
                encoding_step = True
                measured_dn = re.search(r'(measured_loudness|speech\ gated\ loudness)(\=|\:\ )(-?.+)', line)
                measured_dn = round(float(measured_dn[3].strip('.')))
                measured_dn = str(clamp(measured_dn, -31, 0))
                if args.measure_only:
                    pb.update(description=f'[bold cyan]DEE[/bold cyan]: measure | {trim_names(fl_b, 18 + len(measured_dn))} ({measured_dn} dB)', task_id=task_id, completed=100)
                    dee.kill()
                else:
                    pb.update(description=f'[bold cyan]DEE[/bold cyan]: encode | {trim_names(fl_b, 17 + len(measured_dn))} ({measured_dn} dB)', task_id=task_id)

            progress = re.search(r'Stage progress: ([0-9]+\.[0-9])', line)
            if progress and progress[1] != '100.0':
                if aformat != 'thd' and version.parse(simplens.dee_version.replace('-master', '')) >= version.parse('5.2.0') and not encoding_step:
                    pb.update(task_id=task_id, completed=float(progress[1]) / 4)
                else:
                    pb.update(task_id=task_id, completed=float(progress[1]))

            if 'error' in line.lower():
                oprint(line.rstrip().split(': ', 1)[1])
    pb.update(task_id=task_id, completed=100)

    if not args.keeptemp:
        wav_file = os.path.join(config['temp_path'], basename(fl, 'wav'))
        del_time = 0
        while os.path.exists(wav_file) and del_time < 10:
            try:
                os.remove(wav_file)
            except PermissionError:
                # likely dee still use it
                del_time += 1
                time.sleep(1)

        if os.path.exists(wav_file):
            print(f'[bold yellow]Failed to delete:[/bold yellow] {wav_file}')
        os.remove(os.path.join(config['temp_path'], basename(fl, 'xml', sanitize=True)))

    if args.format.lower() == 'thd':
        os.remove(os.path.join(output, basename(fl, 'thd.log')))
        os.remove(os.path.join(output, basename(fl, 'thd.mll')))


def main() -> None:
    if len(sys.argv) == 1:
        parser.print_help(sys.stderr)
        sys.exit(1)

    if args.changelog: print_changelog()
    if args.list_bitrates: list_bitrates()
    if args.print_logos: print_logos()

    if getattr(sys, 'frozen', False):
        script_path = os.path.dirname(sys.executable)
        standalone = 1
    else:
        script_path = os.path.dirname(__file__)
        standalone = 0

    dirs = PlatformDirs('deew', False)
    config_dir_path = dirs.user_config_dir
    config_path1 = os.path.join(config_dir_path, 'config.toml')
    config_path2 = os.path.join(script_path, 'config.toml')

    if args.config:
        if standalone:
            print(f'[bold cyan]Your config locations:[/bold cyan]\n{config_path1}\n{config_path2}\n\n[bold cyan]Your current config:[/bold cyan]')
            if os.path.exists(config_path1):
                current_conf = config_path1
            elif os.path.exists(config_path2):
                current_conf = config_path2
            else:
                print('You don\'t have a config currently.')
                sys.exit(0)
        else:
            print(f'[bold cyan]Your config location:[/bold cyan]\n{config_path1}\n\n[bold cyan]Your current config:[/bold cyan]')
            if os.path.exists(config_path1):
                current_conf = config_path1
            else:
                print('You don\'t have a config currently.')
                sys.exit(0)
        with open(current_conf, 'r') as conf:
            Console().print(Syntax(conf.read(), 'toml'))
        sys.exit(0)

    if args.generate_config:
        generate_config(standalone, config_path1, config_path2, config_dir_path)
        sys.exit(0)

    if not os.path.exists(config_path1) and not os.path.exists(config_path2):
        print(f'[bold yellow]config.toml[/bold yellow] [not bold white]is missing, creating one...[/not bold white]')
        generate_config(standalone, config_path1, config_path2, config_dir_path)

    try:
        config = toml.load(config_path1)
    except Exception:
        config = toml.load(config_path2)
    simplens.config = config

    if 0 < config['logo'] < len(logos) + 1: print(logos[config['logo'] - 1])

    config_keys = [
                    'ffmpeg_path',
                    'ffprobe_path',
                    'dee_path',
                    'temp_path',
                    'logo',
                    'max_instances',
                    'default_bitrates',
                    'summary_sections'
                ]
    c_key_missing = []
    for c_key in config_keys:
        if c_key not in config: c_key_missing.append(c_key)
    if len(c_key_missing) > 0: print_exit('config_key', f'[bold yellow]{"[not bold white], [/not bold white]".join(c_key_missing)}[/bold yellow]')

    for i in config['dee_path'], config['ffmpeg_path'], config['ffprobe_path']:
        if not shutil.which(i): print_exit('binary_exist', i)

    with open(shutil.which(config['dee_path']), 'rb') as fd:
        simplens.dee_is_exe = fd.read(2) == b'\x4d\x5a'
    simplens.is_nonnative_exe = simplens.dee_is_exe and platform.system() != 'Windows'

    if not config['temp_path']:
        if simplens.is_nonnative_exe:
            config['temp_path'] = rwpc(ntpath.join(
                subprocess.run(['powershell.exe', "(gi $env:TEMP).fullname"], capture_output=True, encoding='utf-8').stdout.strip(),
                'deew',
            ))
        else:
            config['temp_path'] = os.path.join(script_path, 'temp') if standalone else tempfile.gettempdir()
            if config['temp_path'] == '/tmp':
                config['temp_path'] = '/var/tmp/deew'
    config['temp_path'] = os.path.abspath(config['temp_path'])
    createdir(config['temp_path'])

    cpu__count = cpu_count()
    if args.instances:
        instances = args.instances
    else:
        instances = config['max_instances']
    if isinstance(instances, str) and instances.endswith('%'):
        instances = cpu__count * (int(instances.replace('%', '')) / 100)
    else:
        instances = int(instances)
    if simplens.dee_is_exe:
        instances = clamp(instances, 1, cpu__count - 2)
        instances = clamp(instances, 1, 6)
    else:
        instances = clamp(instances, 1, cpu__count)
    if instances == 0: instances = 1

    aformat = args.format.lower()
    bitrate = args.bitrate
    downmix = args.downmix
    args.dialnorm = clamp(args.dialnorm, -31, 0)
    trackindex = max(0, args.track_index)

    if aformat not in ['dd', 'ddp', 'thd', 'ac4']: print_exit('format')
    if downmix and downmix not in [1, 2, 6]: print_exit('downmix')
    if downmix and aformat == 'thd': print_exit('thd_downmix')
    if args.drc not in ['film_light', 'film_standard', 'music_light', 'music_standard', 'speech', 'none']: print_exit('drc')
    if not simplens.dee_is_exe and platform.system() == 'Linux' and aformat == 'thd': print_exit('linux_thd')
    if args.measure_only: aformat = 'ddp'

    filelist = []
    for f in args.input:
        if not os.path.exists(f): print_exit('path', f)
        if os.path.isdir(f):
            filelist.extend(glob(f + os.path.sep + '*'))
        else:
            filelist.append(f)

    samplerate_list = []
    channels_list = []
    bit_depth_list = []
    length_list = []

    for f in filelist:
        probe_args = [config["ffprobe_path"], '-v', 'quiet', '-select_streams', f'a:{trackindex}', '-print_format', 'json', '-show_format', '-show_streams', f]
        try:
            output = subprocess.check_output(probe_args, encoding='utf-8')
        except subprocess.CalledProcessError:
            print_exit('ffprobe')
        audio = json.loads(output)['streams'][0]
        samplerate_list.append(int(audio['sample_rate']))
        channels_list.append(audio['channels'])
        length_list.append(float(audio.get('duration', -1)))
        depth = int(audio.get('bits_per_sample', 0))
        if depth == 0: depth = int(audio.get('bits_per_raw_sample', 32))
        bit_depth_list.append(depth)

    if not samplerate_list.count(samplerate_list[0]) == len(samplerate_list): print_exit('sample_mismatch', samplerate_list)
    if not channels_list.count(channels_list[0]) == len(channels_list): print_exit('channel_mismatch', channels_list)
    if not bit_depth_list.count(bit_depth_list[0]) == len(bit_depth_list): print_exit('bitdepth_mismatch', bit_depth_list)

    channels = channels_list[0]
    samplerate = samplerate_list[0]
    bit_depth = bit_depth_list[0]
    if bit_depth not in [16, 24, 32]:
        if bit_depth < 16:
            bit_depth = 16
        elif 16 < bit_depth < 24:
            bit_depth = 24
        else:
            bit_depth = 32

    if channels not in [1, 2, 6, 8]: print_exit('channels')
    if downmix and downmix >= channels: print_exit('downmix_mismatch')
    if not downmix and aformat == 'dd' and channels == 8: downmix = 6
    if aformat == 'ac4' and channels != 6: print_exit('ac4_input_channels')
    if aformat == 'ac4': downmix = 2
    if aformat == 'thd' and channels == 1: print_exit('thd_mono_input')

    downmix_config = 'off'
    if downmix:
        outchannels = downmix
        downmix_config = channel_number_to_name(outchannels)
    else:
        outchannels = channels

    if outchannels in [1, 2] and aformat in ['dd', 'ddp'] and not args.measure_only:
        if args.no_prompt:
            print('Consider using [bold cyan]qaac[/bold cyan] or [bold cyan]opus[/bold cyan] for \
[bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding.')
        else:
            continue_enc = Confirm.ask('Consider using [bold cyan]qaac[/bold cyan] or [bold cyan]opus[/bold cyan] for \
[bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding, are you sure you want to use [bold cyan]DEE[/bold cyan]?')
            if not continue_enc: sys.exit(1)

    if outchannels == 2 and aformat == 'thd':
        if args.no_prompt:
            print('Consider using [bold cyan]FLAC[/bold cyan] for lossless \
[bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding.')
        else:
            continue_enc = Confirm.ask('Consider using [bold cyan]FLAC[/bold cyan] for lossless \
[bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding, are you sure you want to use [bold cyan]DEE[/bold cyan]?')
            if not continue_enc: sys.exit(1)

    if args.dialnorm != 0:
        if args.no_prompt:
            print('Consider leaving the dialnorm value at 0 (auto), setting it manually can be dangerous.')
        else:
            continue_enc = Confirm.ask('Consider leaving the dialnorm value at 0 (auto), setting it manually can be dangerous, are you sure you want to do it?')
            if not continue_enc: sys.exit(1)

    if aformat == 'dd':
        if outchannels == 1:
            if not bitrate: bitrate = config['default_bitrates']['dd_1_0']
            bitrate = find_closest_allowed(bitrate, allowed_bitrates['dd_10'])
        elif outchannels == 2:
            if not bitrate: bitrate = config['default_bitrates']['dd_2_0']
            bitrate = find_closest_allowed(bitrate, allowed_bitrates['dd_20'])
        elif outchannels == 6:
            if not bitrate: bitrate = config['default_bitrates']['dd_5_1']
            bitrate = find_closest_allowed(bitrate, allowed_bitrates['dd_51'])
    if aformat == 'ddp':
        if outchannels == 1:
            if not bitrate: bitrate = config['default_bitrates']['ddp_1_0']
            bitrate = find_closest_allowed(bitrate, allowed_bitrates['ddp_10'])
        elif outchannels == 2:
            if not bitrate: bitrate = config['default_bitrates']['ddp_2_0']
            bitrate = find_closest_allowed(bitrate, allowed_bitrates['ddp_20'])
        elif outchannels == 6:
            if not bitrate: bitrate = config['default_bitrates']['ddp_5_1']
            bitrate = find_closest_allowed(bitrate, allowed_bitrates['ddp_51'])
        elif outchannels == 8:
            if not bitrate: bitrate = config['default_bitrates']['ddp_7_1']
            if args.force_standard:
                bitrate = find_closest_allowed(bitrate, allowed_bitrates['ddp_71_standard'])
            elif args.force_bluray:
                bitrate = find_closest_allowed(bitrate, allowed_bitrates['ddp_71_bluray'])
            else:
                bitrate = find_closest_allowed(bitrate, allowed_bitrates['ddp_71_combined'])
    elif aformat == 'ac4':
        if not bitrate: bitrate = config['default_bitrates']['ac4_2_0']
        bitrate = find_closest_allowed(bitrate, allowed_bitrates['ac4_20'])

    if args.output:
        createdir(os.path.abspath(args.output))
        output = os.path.abspath(args.output)
    else:
        output = os.getcwd()

    if aformat in ['dd', 'ddp']:
        xml_base = xmltodict.parse(xml_dd_ddp_base)
        xml_base['job_config']['output']['ec3']['storage']['local']['path'] = wpc(output, quote=True)
        if aformat == 'ddp':
            xml_base['job_config']['filter']['audio']['pcm_to_ddp']['encoder_mode'] = 'ddp'
            if outchannels == 8:
                xml_base['job_config']['filter']['audio']['pcm_to_ddp']['encoder_mode'] = 'ddp71'
                if bitrate > 1024:
                    xml_base['job_config']['filter']['audio']['pcm_to_ddp']['encoder_mode'] = 'bluray'
                if args.force_standard:
                    xml_base['job_config']['filter']['audio']['pcm_to_ddp']['encoder_mode'] = 'ddp71'
                if args.force_bluray:
                    xml_base['job_config']['filter']['audio']['pcm_to_ddp']['encoder_mode'] = 'bluray'
        if aformat == 'dd':
            xml_base['job_config']['filter']['audio']['pcm_to_ddp']['encoder_mode'] = 'dd'
        xml_base['job_config']['filter']['audio']['pcm_to_ddp']['downmix_config'] = downmix_config
        xml_base['job_config']['filter']['audio']['pcm_to_ddp']['data_rate'] = bitrate
        xml_base['job_config']['filter']['audio']['pcm_to_ddp']['drc']['line_mode_drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['pcm_to_ddp']['drc']['rf_mode_drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['pcm_to_ddp']['custom_dialnorm'] = args.dialnorm
    elif aformat in ['ac4']:
        xml_base = xmltodict.parse(xml_ac4_base)
        xml_base['job_config']['output']['ac4']['storage']['local']['path'] = wpc(output, quote=True)
        xml_base['job_config']['filter']['audio']['encode_to_ims_ac4']['data_rate'] = bitrate
        xml_base['job_config']['filter']['audio']['encode_to_ims_ac4']['drc']['ddp_drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_ims_ac4']['drc']['flat_panel_drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_ims_ac4']['drc']['home_theatre_drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_ims_ac4']['drc']['portable_hp_drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_ims_ac4']['drc']['portable_spkr_drc_profile'] = args.drc
    elif aformat == 'thd':
        xml_base = xmltodict.parse(xml_thd_base)
        xml_base['job_config']['output']['mlp']['storage']['local']['path'] = wpc(output, quote=True)
        xml_base['job_config']['filter']['audio']['encode_to_dthd']['atmos_presentation']['drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_dthd']['presentation_8ch']['drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_dthd']['presentation_6ch']['drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_dthd']['presentation_2ch']['drc_profile'] = args.drc
        xml_base['job_config']['filter']['audio']['encode_to_dthd']['custom_dialnorm'] = args.dialnorm
    xml_base['job_config']['input']['audio']['wav']['storage']['local']['path'] = wpc(config['temp_path'], quote=True)
    xml_base['job_config']['misc']['temp_dir']['path'] = wpc(config['temp_path'], quote=True)

    simplens.dee_version = parse_version_string([config['dee_path']])
    simplens.ffmpeg_version = parse_version_string([config['ffmpeg_path'], '-version'])
    simplens.ffprobe_version = parse_version_string([config['ffprobe_path'], '-version'])

    if not all(x is False for x in config["summary_sections"].values()):
        summary = Table(title='Encoding summary', title_style='not italic bold magenta', show_header=False)
        summary.add_column(style='green')
        summary.add_column(style='color(231)')

        if config['summary_sections']['deew_info']:
            try:
                r = requests.get('https://api.github.com/repos/pcroland/deew/releases/latest')
                latest_version = json.loads(r.text)['tag_name']
                if version.parse(prog_version) < version.parse(latest_version):
                    latest_version = f'[bold green]{latest_version}[/bold green] !!!'
            except Exception:
                latest_version = "[red]couldn't fetch"
            summary.add_row('[bold cyan]Version', prog_version)
            summary.add_row('[bold cyan]Latest', latest_version, end_section=True)

        if config['summary_sections']['binaries']:
            summary.add_row('[cyan]DEE version', simplens.dee_version)
            summary.add_row('[cyan]ffmpeg version', simplens.ffmpeg_version)
            summary.add_row('[cyan]ffprobe version', simplens.ffprobe_version, end_section=True)

        if config['summary_sections']['input_info']:
            summary.add_row('[bold yellow]Input')
            summary.add_row('Channels', channel_number_to_name(channels))
            summary.add_row('Bit depth', str(bit_depth), end_section=True)

        if not args.measure_only:
            if config['summary_sections']['output_info']:
                summary.add_row('[bold yellow]Output')
                summary.add_row('Format', 'TrueHD' if aformat == 'thd' else aformat.upper())
                summary.add_row('Channels', 'immersive stereo' if aformat == 'ac4' else channel_number_to_name(outchannels))
                summary.add_row('Bitrate', 'N/A' if aformat == 'thd' else f'{str(bitrate)} kbps')
                summary.add_row('Dialnorm', 'auto (0)' if args.dialnorm == 0 else f'{str(args.dialnorm)} dB', end_section=True)

        if config['summary_sections']['other']:
            if args.delay:
                delay_print, delay_xml, delay_mode = convert_delay_to_ms(args.delay, compensate=False)
            summary.add_row('[bold yellow]Other')
            summary.add_row('Files', str(len(filelist)))
            summary.add_row('Max instances', str(f'{instances:g}'))
            summary.add_row('Delay', delay_print if args.delay else '0 ms or parsed from filename')
            summary.add_row('Temp path', config['temp_path'])

        print(summary)
        print()

    resample_value = ''
    if aformat in ['dd', 'ddp', 'ac4'] and samplerate != 48000:
        bit_depth = 32
        resample_value = '48000'
    elif aformat == 'thd' and samplerate not in [48000, 96000]:
        bit_depth = 32
        if samplerate < 72000:
            resample_value = '48000'
        else:
            resample_value = '96000'
    if resample_value:
        if channels == 8:
            channel_swap = 'pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5,'
        else:
            channel_swap = ''
        resample_args = [
            '-filter_complex', f'[a:{trackindex}]{channel_swap}aresample=resampler=soxr',
            '-ar', resample_value,
            '-precision', '28',
            '-cutoff', '1',
            '-dither_scale', '0']
        resample_args_print = f'-filter_complex [bold color(231)]"\[a:{trackindex}]{channel_swap}aresample=resampler=soxr"[/bold color(231)] \
-ar [bold color(231)]{resample_value}[/bold color(231)] \
-precision [bold color(231)]28[/bold color(231)] \
-cutoff [bold color(231)]1[/bold color(231)] \
-dither_scale [bold color(231)]0[/bold color(231)] '
    else:
        resample_args = []
        resample_args_print = ''

    if channels == 8 and not resample_args:
        channel_swap_args = ['-filter_complex', f'[a:{trackindex}]pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5']
        channel_swap_args_print = f'-filter_complex [bold color(231)]"\[a:{trackindex}]pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5"[/bold color(231)] '
    else:
        channel_swap_args = []
        channel_swap_args_print = ''

    if simplens.is_nonnative_exe:
        dee_xml_input_base = wpc(config['temp_path'])
    else:
        dee_xml_input_base = config['temp_path'] if config['temp_path'].endswith('/') else f'{config["temp_path"]}/'

    xml_validation = [] if simplens.dee_is_exe else ['--disable-xml-validation']
    xml_validation_print = '' if simplens.dee_is_exe else ' --disable-xml-validation'

    if '-filter_complex' in resample_args or '-filter_complex' in channel_swap_args:
        map_args = []
        map_args_print = ''
    else:
        map_args = ['-map', f'0:a:{trackindex}']
        map_args_print = '-map [bold color(231)]0:a[/bold color(231)]' + f'[bold color(231)]:{trackindex}[/bold color(231)] '

    settings = []
    ffmpeg_print_list = []
    dee_print_list = []
    intermediate_exists_list = []
    for i in range(len(filelist)):
        dee_xml_input = f'{dee_xml_input_base}{basename(filelist[i], "xml", sanitize=True)}'

        ffmpeg_args = [
            config['ffmpeg_path'],
            '-y',
            '-drc_scale', '0',
            '-i', filelist[i],
            *(map_args),
            '-c', f'pcm_s{bit_depth}le',
            *(channel_swap_args), *(resample_args),
            '-rf64', 'always',
            os.path.join(config['temp_path'], basename(filelist[i], 'wav'))
        ]
        dee_args = [
            config['dee_path'],
            '--progress-interval', '500',
            '--diagnostics-interval', '90000',
            '-x', dee_xml_input,
            *(xml_validation)
        ]

        ffmpeg_args_print = f'[bold cyan]ffmpeg[/bold cyan] \
-y \
-drc_scale [bold color(231)]0[/bold color(231)] \
-i [bold green]{filelist[i]}[/bold green] \
{map_args_print}\
-c [bold color(231)]pcm_s{bit_depth}le[/bold color(231)] \
{channel_swap_args_print}{resample_args_print}\
-rf64 [bold color(231)]always[/bold color(231)] \
[bold magenta]{os.path.join(config["temp_path"], basename(filelist[i], "wav"))}[/bold magenta]'

        ffmpeg_args_print_short = f'[bold cyan]ffmpeg[/bold cyan] \
-y \
-drc_scale [bold color(231)]0[/bold color(231)] \
-i [bold green]\[input][/bold green] \
{map_args_print}\
-c [bold color(231)]pcm_s{bit_depth}le[/bold color(231)] \
{channel_swap_args_print}{resample_args_print}\
-rf64 [bold color(231)]always[/bold color(231)] \
[bold magenta]\[output][/bold magenta]'

        dee_args_print = f'[bold cyan]dee[/bold cyan] -x [bold magenta]{dee_xml_input}[/bold magenta]{xml_validation_print}'
        dee_args_print_short = f'[bold cyan]dee[/bold cyan] -x [bold magenta]\[input][/bold magenta]{xml_validation_print}'

        intermediate_exists = False
        if os.path.exists(os.path.join(config['temp_path'], basename(filelist[i], 'wav'))):
            intermediate_exists = True
            ffmpeg_args_print = '[green]Intermediate already exists[/green]'

        ffmpeg_print_list.append(ffmpeg_args_print)
        dee_print_list.append(dee_args_print)
        if intermediate_exists: intermediate_exists_list.append(os.path.basename(filelist[i]))

        delay_in_filename = re.match(r'.+DELAY ([-|+]?[0-9]+m?s)\..+', filelist[i])
        if delay_in_filename:
            delay = delay_in_filename[1]
            if not delay.startswith(('-', '+')):
                delay = f'+{delay}'
        else:
            delay = '+0ms'
        if args.delay:
            delay = args.delay

        xml = deepcopy(xml_base)
        xml['job_config']['input']['audio']['wav']['file_name'] = basename(filelist[i], 'wav', quote=True)
        if aformat == 'ddp':
            xml['job_config']['output']['ec3']['file_name'] = basename(filelist[i], 'ec3', quote=True, stripdelay=True)
            if bitrate > 1024:
                xml['job_config']['output']['ec3']['file_name'] = basename(filelist[i], 'eb3', quote=True, stripdelay=True)
            if args.force_standard:
                xml['job_config']['output']['ec3']['file_name'] = basename(filelist[i], 'ec3', quote=True, stripdelay=True)
            if args.force_bluray:
                xml['job_config']['output']['ec3']['file_name'] = basename(filelist[i], 'eb3', quote=True, stripdelay=True)
        elif aformat == 'dd':
            xml['job_config']['output']['ec3']['file_name'] = basename(filelist[i], 'ac3', quote=True, stripdelay=True)
            xml['job_config']['output']['ac3'] = xml['job_config']['output']['ec3']
            del xml['job_config']['output']['ec3']
        elif aformat == 'ac4':
            xml['job_config']['output']['ac4']['file_name'] = basename(filelist[i], 'ac4', quote=True, stripdelay=True)
        else:
            xml['job_config']['output']['mlp']['file_name'] = basename(filelist[i], 'thd', quote=True, stripdelay=True)

        if aformat in ['dd', 'ddp']:
            delay_print, delay_xml, delay_mode = convert_delay_to_ms(delay, compensate=True)
            xml['job_config']['filter']['audio']['pcm_to_ddp'][delay_mode] = delay_xml
        elif aformat in ['ac4']:
            delay_print, delay_xml, delay_mode = convert_delay_to_ms(delay, compensate=True)
            xml['job_config']['filter']['audio']['encode_to_ims_ac4'][delay_mode] = delay_xml
        else:
            delay_print, delay_xml, delay_mode = convert_delay_to_ms(delay, compensate=False)
            xml['job_config']['filter']['audio']['encode_to_dthd'][delay_mode] = delay_xml

        save_xml(os.path.join(config['temp_path'], basename(filelist[i], 'xml', sanitize=True)), xml)

        settings.append([filelist[i], output, length_list[i], ffmpeg_args, dee_args, intermediate_exists, aformat])

    if args.long_argument:
        print('[bold color(231)]Running the following commands:[/bold color(231)]')
        for ff, d in zip(ffmpeg_print_list, dee_print_list):
            print(f'{ff} && {d}')
    else:
        if intermediate_exists_list:
            print(f'[bold color(231)]Intermediate already exists for the following file(s):[/bold color(231)] \
[bold magenta]{"[not bold white],[/not bold white] ".join(intermediate_exists_list)}[bold magenta]')
        print(f'[bold color(231)]Running the following commands:[/bold color(231)]\n{ffmpeg_args_print_short} && {dee_args_print_short}')

    print()

    pb = Progress('[', '{task.description}', ']', BarColumn(), '[magenta]{task.percentage:>3.2f}%', refresh_per_second=8)
    simplens.pb = pb

    signal.signal(signal.SIGINT, signal.SIG_DFL)

    jobs = []
    with pb:
        with ThreadPoolExecutor(max_workers=instances) as pool:
            for setting in settings:
                task_id = pb.add_task('', visible=False, total=None)
                jobs.append(pool.submit(encode, task_id, setting))
    for job in jobs:
        job.result()


if __name__ == '__main__':
    main()
