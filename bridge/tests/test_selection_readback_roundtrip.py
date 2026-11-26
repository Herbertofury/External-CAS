"""Combined bridge fixture; no game import, real save, or background service."""
import io
import unittest

from test_read_adapter import FrameCodec, GameThreadBridge, Sims4ReadAdapter
from test_selection_initial_state import SelectionServices


class SelectionReadbackRoundtrip(unittest.TestCase):
    def test_connect_snapshot_and_failed_readback_preserve_identity(self):
        services = SelectionServices()
        secret, nonce = b'x' * 32, b'y' * 16
        adapter = Sims4ReadAdapter(services, 'fixture', nonce)
        bridge = GameThreadBridge(adapter)
        desktop, game = FrameCodec(secret, nonce, 0), FrameCodec(secret, nonce, 1)
        observations = []

        def to_desktop(message):
            return desktop.read(io.BytesIO(game.encode(message)))

        def request(message):
            bridge.enqueue(game.read(io.BytesIO(desktop.encode(message))))
            self.assertEqual(bridge.pump(budget_ms=100), 1)
            return to_desktop(bridge.outbox.get_nowait())

        adapter.attach_selection(lambda message: observations.append(to_desktop(message)))
        first = observations[0]['body']
        self.assertEqual(first['sequence'], 1)
        identity = first['identity']
        self.assertEqual(identity['sim_id'], 7)
        ticket = {'session_nonce': list(nonce), 'identity': identity,
                  'navigation_epoch': 1, 'request_id': 1}
        snapshot = request({'kind': 'SnapshotRequest', 'body': {'ticket': ticket}})
        self.assertEqual(snapshot['kind'], 'Snapshot')
        data = snapshot['body']['snapshot']
        self.assertEqual(data['identity'], identity)
        self.assertEqual(bytes(data['fields']['outfits']), b'raw-unknown-outfits')

        operation = list(range(16))
        target = {**identity, 'sim_id': 99}
        refused = request({'kind': 'ReadbackRequest', 'body': {
            'operation_id': operation, 'session_nonce': list(nonce), 'identity': target}})
        self.assertEqual(refused['kind'], 'Refused')
        self.assertEqual(refused['body']['operation_id'], operation)

        readback = request({'kind': 'ReadbackRequest', 'body': {
            'operation_id': operation, 'session_nonce': list(nonce), 'identity': identity}})
        self.assertEqual(readback['kind'], 'Readback')
        self.assertEqual(readback['body']['operation_id'], operation)
        self.assertEqual(readback['body']['snapshot'], data)
        adapter.detach()
        self.assertEqual(services.client.listeners, [])
