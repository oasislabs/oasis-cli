"""Tests `oasis set-toolchain`."""

import os
import os.path as osp
from subprocess import PIPE


def test_profile_option(oenv, mock_tool):
    mock_npm = osp.join(oenv.bin_dir, 'npm')
    with open(mock_npm, 'w') as f_mock_npm:
        f_mock_npm.write(mock_tool.create())
    os.chmod(mock_npm, int('755', 8))

    # note below: 0th invocation of "npm" is `npm install`, which is done without OASIS_PROFILE

    app_dir = osp.join(oenv.create_project(), 'app')

    oenv.run('oasis config profile.default.private_key ""')

    cp = oenv.run('oasis test', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[1]['env']['OASIS_PROFILE'] == 'local'

    cp = oenv.run('oasis test --profile default', cwd=app_dir, stdout=PIPE)
    assert mock_tool.parse_output(cp.stdout)[1]['env']['OASIS_PROFILE'] == 'default'

    cp = oenv.run('oasis test --profile oasisbook', cwd=app_dir, stderr=PIPE, check=False)
    assert '`profile.oasisbook` does not exist' in cp.stderr
