"""Tests `oasis test`."""

import os.path as osp
from subprocess import PIPE


def test_invoke_npm(oenv, mock_tool):
    mock_tool.create_at(osp.join(oenv.bin_dir, 'npm'))
    app_dir = osp.join(oenv.create_project(), 'app')

    cp = oenv.run('oasis test', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[0]['args'][0] == 'install'
    assert mock_tool.parse_output(cp.stdout)[1]['args'][0] == 'test'


def test_alt_npm(oenv, mock_tool):
    mock_tool.create_at(osp.join(oenv.bin_dir, 'yarn'))
    app_dir = osp.join(oenv.create_project(), 'app')
    oenv.run('oasis test', env={'OASIS_NPM': 'yarn'}, cwd=app_dir)


def test_test_profile(oenv, mock_tool):
    mock_tool.create_at(osp.join(oenv.bin_dir, 'npm'))
    app_dir = osp.join(oenv.create_project(), 'app')

    # note below: 0th invocation of "npm" is `npm install`, which is done without OASIS_PROFILE

    cp = oenv.run('oasis test', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[1]['env']['OASIS_PROFILE'] == 'local'

    oenv.run('oasis config profile.default.private_key ""')
    cp = oenv.run('oasis test --profile default', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[1]['env']['OASIS_PROFILE'] == 'default'

    cp = oenv.run('oasis test --profile oasisbook', cwd=app_dir, stderr=PIPE, check=False)
    assert '`profile.oasisbook` does not exist' in cp.stderr
