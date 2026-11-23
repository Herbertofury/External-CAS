"""Selection observer fixture; no Sims imports or live session."""
import unittest

from test_read_adapter import Services
from external_cas_bridge.sims4_read_adapter import Sims4ReadAdapter


class ImmediateClient:
    def __init__(self):
        self.listeners = []

    def register_active_sim_changed(self, callback):
        self.listeners.append(callback)
        callback()

    def unregister_active_sim_changed(self, callback):
        self.listeners.remove(callback)


class SelectionServices(Services):
    def __init__(self):
        self.client = ImmediateClient()
        self.sim = super().active_sim_info()

    def client_manager(self):
        return self

    def get_first_client(self):
        return self.client

    def active_sim_info(self):
        return self.sim


class SelectionTests(unittest.TestCase):
    def test_attach_publishes_current_sim_once_and_keeps_sequence_on_reattach(self):
        services = SelectionServices()
        adapter = Sims4ReadAdapter(services, 'fixture', b'x' * 16)
        observations = []
        adapter.attach_selection(observations.append)
        self.assertEqual(observations, [{
            'kind': 'ObserveSelection',
            'body': {'sequence': 1, 'identity': adapter.selected()},
        }])
        services.sim = None
        services.client.listeners[0]()
        self.assertEqual(observations[-1]['body'], {'sequence': 2, 'identity': None})
        detached_listener = services.client.listeners[0]
        adapter.detach()
        self.assertEqual(services.client.listeners, [])
        detached_listener()
        adapter.attach_selection(observations.append)
        self.assertEqual([event['body']['sequence'] for event in observations], [1, 2, 3])
        self.assertIsNone(observations[-1]['body']['identity'])

    def test_attach_undoes_subscription_when_initial_delivery_fails(self):
        services = SelectionServices()
        adapter = Sims4ReadAdapter(services, 'fixture', b'x' * 16)

        def fail(_message):
            raise RuntimeError('fixture receiver unavailable')

        with self.assertRaisesRegex(RuntimeError, 'fixture receiver unavailable'):
            adapter.attach_selection(fail)
        self.assertEqual(services.client.listeners, [])
        self.assertIsNone(adapter.client)
        seen = []
        adapter.attach_selection(seen.append)
        self.assertEqual(len(seen), 1)
        self.assertEqual(seen[0]['body']['identity']['sim_id'], 7)
