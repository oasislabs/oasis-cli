"""An HTTP server that mocks https://tools.oasis.dev. Used by test_cmd_set_toolchain.py."""

import http.server
from xml.etree import ElementTree as ET

from conftest import MockTool

PLATFORMS = ['linux', 'darwin']
TOOLS = ['oasis', 'oasis-chain']
RELEASE_HASHES = [('19.20', 'abcdef0'), ('20.19', '0fedcba'), ('current', '1111111'),
                  ('cache', '2222222')]


def _get_manifest():
    root = ET.Element('ListBucketResult', xmlns="http://s3.amazonaws.com/doc/2006-03-01/")
    ET.SubElement(root, 'Name').text = 'tools.oasis.dev'
    ET.SubElement(root, 'Prefix')
    ET.SubElement(root, 'Marker')
    ET.SubElement(root, 'MaxKeys').text = '1000'
    ET.SubElement(root, 'IsTruncated').text = 'false'

    def _gen_contents(key):
        contents = ET.SubElement(root, 'Contents')
        ET.SubElement(contents, 'Key').text = key
        ET.SubElement(contents, 'LastModified').text = "2019-08-05T23:59:16.000Z"
        ET.SubElement(contents, 'ETag').text = '"94e8c8b44d7aa60920f01f9f1e354fa2-2"'
        ET.SubElement(contents, 'Size').text = '42'
        ET.SubElement(contents, 'StorageClass').text = 'STANDARD'
        return contents

    contents = []

    # ^ hashes are not per-release, but it'll simplify testing
    for platform in PLATFORMS:
        for release_name, thash in RELEASE_HASHES:
            release_str = release_name
            if release_name not in {'current', 'cache'}:
                release_str = f'release/{release_name}'
            for tool in TOOLS:
                contents.append(_gen_contents(f'{platform}/{release_str}/{tool}-{thash}'))

    for extra in ['successful_builds', 'some_other_file']:
        contents.append(_gen_contents(extra))

    return ET.tostring(root)


class MockToolsHandler(http.server.BaseHTTPRequestHandler):
    """An HTTP server handler that mocks https://tools.oasis.dev"""

    URL = 'http://tools.oasis.dev'

    def do_GET(self):  # pylint:disable=invalid-name
        """Handles a GET request in a manner that similar to an s3 bucket.
            / - returns the bucket keys
            /<key> - returns the value stored at <key>
        """
        if not self.path.startswith(self.URL):
            self.send_response(404)
            self.end_headers()
            return
        path = self.path[len(self.URL):]
        if path == '/':
            manifest = _get_manifest()
            self.send_response(200)
            self.send_header('Content-Type', 'text/xml')
            self.send_header('Content-Length', len(manifest))
            self.end_headers()
            self.wfile.write(manifest)
            return

        exists = True
        path_comps = path[1:].split('/')
        if len(path_comps) == 4:  # /<platform>/release/<release>/<tool>
            (platform, lit_release, release, tool_hash) = path_comps
            exists = lit_release == 'release'
        elif len(path_comps) == 3:  # /<platform>/current/<tool>
            (platform, release, tool_hash) = path_comps
        else:
            exists = False

        try:
            tool, thash = tool_hash.rsplit('-', 1)
        except ValueError:
            exists = False
        exists &= (platform in PLATFORMS and tool in TOOLS and (release, thash) in RELEASE_HASHES)

        if not exists:
            self.send_response(404)
            self.send_header('Content-Length', 0)
            self.end_headers()
            return

        mock_tool = MockTool().create(f'echo "{platform} {release} {tool} {thash}"')
        mock_tool = mock_tool.encode('utf8')
        self.send_response(200)
        self.send_header('Content-Type', 'binary/octet-stream')
        self.send_header('Content-Length', len(mock_tool))
        self.end_headers()
        self.wfile.write(mock_tool)
        return


def main():
    host = 'localhost'
    port = 8080
    while True:
        try:
            server = http.server.HTTPServer((host, port), MockToolsHandler)
            print(port, flush=True)
            server.serve_forever()
        except OSError as err:
            if err.errno == 48 or err.errno == 98:  # eaddr
                port += 1
            else:
                raise err


if __name__ == '__main__':
    main()
