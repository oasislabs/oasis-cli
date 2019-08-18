"""Tests `oasis deploy`."""

import os.path as osp
from subprocess import PIPE

from .conftest import SAMPLE_KEY


def test_deploy_no_key(oenv):
    app_dir = osp.join(oenv.create_project(), 'app')
    cp = oenv.run('oasis deploy', cwd=app_dir, stdout=PIPE, check=False)
    assert 'https://dashboard.oasiscloud.io' in cp.stdout


def test_deploy_with_key(oenv, mock_tool):
    mock_tool.create_at(osp.join(oenv.bin_dir, 'npm'))
    app_dir = osp.join(oenv.create_project(), 'app')
    oenv.run(f'oasis config profile.default.credential "{SAMPLE_KEY}"')
    cp = oenv.run('oasis deploy', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[1]['env']['OASIS_PROFILE'] == 'default'


def test_deploy_profile(oenv, mock_tool):
    mock_tool.create_at(osp.join(oenv.bin_dir, 'npm'))
    app_dir = osp.join(oenv.create_project(), 'app')

    # note below: 0th invocation of "npm" is `npm install`, which is done without OASIS_PROFILE

    cp = oenv.run('oasis deploy --profile local', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[1]['env']['OASIS_PROFILE'] == 'local'
