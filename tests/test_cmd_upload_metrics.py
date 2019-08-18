"""Tests `oasis upload_telemetry`."""

import http.client
import os.path as osp
import subprocess
from subprocess import PIPE
import sys
import uuid

import pytest

MOCK_SERVER_PY = osp.join(osp.dirname(__file__), 'mock_telemetry_server.py')


@pytest.fixture(scope='package')
def telemetry_proxy():
    """Starts an HTTP server that mocks https://telemetry.oasiscloud.io"""
    cp = subprocess.Popen([sys.executable, MOCK_SERVER_PY], stdout=PIPE)
    port = cp.stdout.readline().rstrip().decode('utf8')
    yield f'http://localhost:{port}'
    cp.terminate()
    cp.wait()


def test_upload_metrics(oenv, telemetry_proxy):
    oenv.telemetry_config()
    env = {'http_proxy': telemetry_proxy}

    def _run_upload():
        oenv.run('oasis upload_metrics', input='', env=env)
        conn = http.client.HTTPConnection(telemetry_proxy.replace('http://', ''))
        conn.request('GET', '')
        return conn.getresponse().read().decode('utf8')

    event = '{ "event": "did the thing" }'
    with open(oenv.metrics_file, 'w') as f_metrics:
        f_metrics.write(event)
        f_metrics.write('\n')

    [user_id, submitted_event] = _run_upload().rstrip().split('\n')
    assert str(uuid.UUID(user_id)) == user_id

    assert submitted_event == event

    [user_id] = _run_upload().rstrip().split('\n')
    assert str(uuid.UUID(user_id)) == user_id
