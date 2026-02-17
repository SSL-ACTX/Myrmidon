# tests/test_distribution_stress.py
import asyncio
import multiprocessing
import time
import myrmidon

ADDR = "127.0.0.1:9005"
SERVICE_NAME = "heavy-worker"

def run_provider_node():
    """ Node B: The Worker Node """
    rt = myrmidon.Runtime()
    rt.listen(ADDR)

    def initial_logic(msg):
        # Phase 4/5: Initial logic just acknowledges
        pass

    pid = rt.spawn(initial_logic)
    rt.register(SERVICE_NAME, pid)
    print(f"🏗️  Provider: Service '{SERVICE_NAME}' registered at {pid}")

    # Keep alive for the test duration
    time.sleep(2)

    # Trigger a live upgrade while clients are likely querying
    def upgraded_logic(msg):
        # New logic could do something with the data
        pass

    print("🔄 Provider: Performing LIVE HOT-SWAP...")
    rt.hot_swap(pid, upgraded_logic)

    time.sleep(5)
    print("💀 Provider: Shutting down...")

async def stress_client(client_id, rt):
    """ Concurrent client using Phase 7 Async Discovery """
    try:
        # Stress the async resolution membrane
        # This tests the resolve_remote_py -> Rust Future -> Python Awaitable bridge
        pid = await rt.resolve_remote_py(ADDR, SERVICE_NAME)

        if pid:
            # Send high-frequency remote messages
            for i in range(50):
                rt.send_remote(ADDR, pid, f"req-{client_id}-{i}".encode())
                if i % 25 == 0:
                    await asyncio.sleep(0.01) # Yield to event loop
            return True
    except Exception as e:
        print(f"❌ Client {client_id} Error: {e}")
        return False

def run_monitor_node():
    """ Node A: The Orchestrator & Monitor """
    time.sleep(1) # Wait for provider to bind
    rt = myrmidon.Runtime()

    print("🔭 Monitor: Starting Concurrent Async Stress Test...")

    async def main():
        # Spawn 20 concurrent async clients discovering and sending via the Rust bridge
        tasks = [stress_client(i, rt) for i in range(20)]
        results = await asyncio.gather(*tasks)

        success_count = sum(1 for r in results if r)
        print(f"📊 Monitor: {success_count}/20 clients successfully completed async discovery and transmission.")

        # Test Structured System Messages
        # We spawn a local observer to see if the Provider Node's health can be tracked
        target_pid = rt.resolve_remote(ADDR, SERVICE_NAME)
        if target_pid:
            rt.monitor_remote(ADDR, target_pid)
            print(f"🛡️  Monitor: Watching remote PID {target_pid} for EXIT objects...")

            # Wait for the Provider to exit
            start = time.time()
            while time.time() - start < 10:
                if not rt.is_node_up(ADDR):
                    print("🚨 ALERT: Structured monitor detected remote node failure!")
                    break
                time.sleep(0.2)

    asyncio.run(main())

if __name__ == "__main__":
    print(f"--- Myrmidon Stress Test ---")
    p1 = multiprocessing.Process(target=run_provider_node)
    p2 = multiprocessing.Process(target=run_monitor_node)

    p1.start()
    p2.start()

    p2.join()
    p1.terminate()
    print("✨ Stress test complete.")
