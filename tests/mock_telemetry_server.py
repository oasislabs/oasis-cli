"""An HTTP server that mocks https://telemetry.oasiscloud.io. Used by test_cmd_upload_metrics.py."""

import gzip
import http.server


class MockTelemetryHandler(http.server.BaseHTTPRequestHandler):
    """An HTTP server handler that mocks https://telemetry.oasiscloud.io"""

    _uploaded = b''

    def do_GET(self):  # pylint:disable=invalid-name
        """Not part of the actual server API. Returns the last submission."""
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain')
        self.send_header('Content-Length', len(self._uploaded))
        self.end_headers()
        self.wfile.write(MockTelemetryHandler._uploaded)

    def do_POST(self):  # pylint:disable=invalid-name
        """Not part of the actual server API. Returns the last submission."""
        MockTelemetryHandler._uploaded = gzip.decompress(
            self.rfile.read(int(self.headers['Content-Length'])))
        self.send_response(200)
        self.end_headers()


def main():
    host = 'localhost'
    port = 8080
    while True:
        try:
            server = http.server.HTTPServer((host, port), MockTelemetryHandler)
            print(port, flush=True)
            server.serve_forever()
        except OSError as err:
            if err.errno == 48 or err.errno == 98:  # eaddr
                port += 1
            else:
                raise err


if __name__ == '__main__':
    main()
