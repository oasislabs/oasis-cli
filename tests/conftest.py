"""Utilities for testing the Oasis CLI."""

import os
import os.path as osp
import random
import shutil
import string
import subprocess
from subprocess import DEVNULL
import tempfile

import pytest

PROJ_ROOT = osp.abspath(osp.join(osp.dirname(__file__), '..'))
TARGET_DIR = osp.join(PROJ_ROOT, 'target', 'debug')

SAMPLE_KEY = '77827066de994266ffc685a8165e6f1b62c671ff801ba08475ca4c8b41ebf388'
SAMPLE_TOKEN = 'By9Uzva7SezLX+mMnJyUKh/pBOqQhfkuFkUWtkakMRc='
SAMPLE_MNEMONIC = 'range drive remove bleak mule satisfy mandate east lion minimum unfold ready'


@pytest.fixture(params=[None, 'custom_prefix'])
def oenv(request):
    # If this is outside, pytest tries to reuse it.
    class OasisEnv:  # pylint:disable=too-many-instance-attributes
        """Provides information about the virtual user environment in which
           the CLI is currently running."""
        def __init__(self, init_env, home_dir, user_config_dir, user_data_dir):
            self.home_dir = home_dir
            self.config_dir = osp.join(user_config_dir, 'oasis')
            self.data_dir = osp.join(user_data_dir, 'oasis')

            self.bin_dir = osp.join(osp.dirname(user_data_dir), 'bin')
            os.makedirs(self.bin_dir, exist_ok=True)

            self.config_file = osp.join(self.config_dir, 'config.toml')
            self.metrics_file = osp.join(self.data_dir, 'metrics.jsonl')

            self.env = init_env
            git_dir = osp.dirname(shutil.which('git'))
            self.env.update({
                'CARGO_HOME': osp.join(os.environ['HOME'], '.cargo'),
                'RUSTUP_HOME': osp.join(os.environ['HOME'], '.rustup'),
                'HOME': self.home_dir,
                'PATH': f'{self.bin_dir}:{TARGET_DIR}:/usr/bin/:/bin:{git_dir}',
            })

            self._configured = False

        def run(self, cmd, env=None, input='', **kwargs):  # pylint:disable=redefined-builtin
            if not self._configured:
                self.default_config()
            env = env if env else {}
            env.update(self.env)
            return self._run(cmd, env=env, input=input, **kwargs)

        def no_config(self):
            self._configured = True

        def default_config(self):
            self._configure('')

        def telemetry_config(self):
            self._configure('y')

        def create_project(self):
            """Returns the path to a project created using `oasis init`."""
            proj_name = ''.join(random.choice(string.ascii_lowercase) for _ in range(8))
            proj_path = osp.join(self.home_dir, proj_name)
            self.run(f'oasis init {proj_path}')
            return proj_path

        def _configure(self, opts):
            if self._configured:
                return
            self._run('oasis', input=opts, env=self.env, stdout=DEVNULL, stderr=DEVNULL)
            self._configured = True

        def _run(self, cmd, check=True, env=None, cwd=None, **run_args):
            if cwd is None:
                cwd = self.home_dir
            return subprocess.run(
                cmd, cwd=cwd, shell=True, env=env, check=check, encoding='utf8', **run_args)

    # OasisEnv factory generator
    with tempfile.TemporaryDirectory() as tempdir:
        init_env = {}
        if request.param:
            config_dir = osp.abspath(osp.join(tempdir, request.param, 'config'))
            data_dir = osp.abspath(osp.join(tempdir, request.param, 'data'))
            init_env['XDG_CONFIG_HOME'] = config_dir
            init_env['XDG_DATA_HOME'] = data_dir
        else:
            config_dir = osp.join(tempdir, '.config')
            data_dir = osp.join(tempdir, '.local', 'share')

        yield OasisEnv(init_env, tempdir, config_dir, data_dir)


class MockTool:
    """Factory for mock tool binaries and utilities for parsing their output."""
    def __init__(self):
        self.mock_tool_path = osp.join(osp.dirname(__file__), 'res', 'mock_tool.sh')
        with open(self.mock_tool_path) as f_mock_tool:
            self.mock_tool = f_mock_tool.read()

    def create(self, user_script=''):
        return self.mock_tool.replace('$user_script', user_script)

    def create_at(self, path):
        """Creates an executable mock tool at the provied location."""
        shutil.copy(self.mock_tool_path, path)

    @staticmethod
    def parse_output(output):
        """Parses a mock tool output into
            invocations = [ { name: str, envs: dict, args: [str], user: str } ]"""
        invocations = []
        invocation = {}
        has_envs = False
        has_args = False
        for line in output.split('\n'):
            line = line.strip()
            if not line:
                continue
            if not invocation and line.startswith('BEGIN MOCK'):
                invocation = {'name': line.split(' ', 2)[-1], 'env': {}, 'args': [], 'user': ''}
            elif invocation and not has_envs:
                if line == '---':
                    has_envs = True
                else:
                    k, v = line.split('=', 1)
                    invocation['env'][k] = v
            elif invocation and not has_args:
                if line == '---':
                    has_args = True
                else:
                    invocation['args'].append(line)
            elif invocation and has_args:
                if line.startswith('END MOCK'):
                    invocation['user'] = invocation['user'].rstrip()
                    invocations.append(invocation)
                    invocation = {}
                    has_envs = False
                    has_args = False
                else:
                    invocation['user'] += line + '\n'
        return invocations


@pytest.fixture(scope='package')
def mock_tool():
    return MockTool()


def _mtime(path):
    return max(
        max(osp.getmtime(osp.join(dir_name, f)) for f in files)
        for dir_name, _, files in os.walk(path) if files)


def _cargo_build():
    src_mtime = _mtime(osp.join(PROJ_ROOT, 'src'))
    target_mtime = osp.getmtime(osp.join(TARGET_DIR, 'oasis'))
    if src_mtime > target_mtime:
        subprocess.run(['cargo', 'build'], check=True)


_cargo_build()
