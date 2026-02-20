# Myrmidon Benchmarks

The following benchmarks demonstrate Myrmidon's raw throughput and memory efficiency. To test the extreme boundaries of the runtime's memory footprint, synchronization overhead, and cache locality, these benchmarks were run on **severely constrained hardware** rather than typical server infrastructure.

If Myrmidon can juggle 100,000 concurrent actors on a phone and a 15-year-old laptop, it will fly on modern server architecture.

## Methodology

We test the two primary actor paradigms supported by Myrmidon:
1. **Push Actors (Green Threads):** Pure Tokio futures. The Rust runtime pushes messages directly to the callbacks. Highly efficient, zero-cost abstractions.
2. **Pull Actors (Threaded / OS Threads):** Standard "Erlang-style" blocking actors. The runtime provisions a dedicated OS thread (via Tokio's blocking pool) where the interpreter (e.g., Python) waits on a `recv()` call, releasing the GIL.

**The Goal:** Spawn up to 100,000 actors, blitz them with messages, and perform a clean teardown.

---

## Test Environment 1: The Modern Edge (Android Smartphone)

**Hardware:** Infinix X669 (Unisoc T606 - 8 Cores @ 1.61 GHz, 4GB RAM)
**Environment:** Termux, Ubuntu PRoot, Python 3.10

This test validates Myrmidon's ability to allocate and schedule massive concurrency on low-power ARM architecture.

### Results

| Test Type | Target Actors | Spawn Rate | Message Throughput | Time to Send |
| :--- | :--- | :--- | :--- | :--- |
| **Pull (Threaded)** | 50,000 | ~113,841 actors/sec | **~391,111 msgs/sec** | 0.13s |
| **Push (Green)** | 100,000 | ~96,812 actors/sec | **~337,913 msgs/sec** | 0.30s |

#### Analysis: Memory and Allocation
Spawning 50,000 dedicated OS-thread backed actors on a mobile device without triggering the Linux OOM (Out-Of-Memory) killer validates the extreme leanness of the `Mailbox` architecture. The spawn rate on the 8-core ARM chip scales beautifully, easily breaking 100k allocations per second. 

<details>
<summary><strong>View Raw Output (Push / 100k)</strong></summary>

```text
--- Myrmidon Stress Test: 0.1.3 ---
Goal: Spawn 100000 actors.
🚀 Spawning...
   [10000/100000] Spawned. Rate: 127539 actors/sec
   ...
✅ Spawned 100000 actors in 1.03s (96812 actors/sec)
📨 Sending 100000 messages...
✅ Sent 100000 messages in 0.30s (337913 msgs/sec)
🛑 Cleaning up (stopping all actors)...
Done.

```

</details>

---

## Test Environment 2: The Legacy Constraint (15-Year-Old Laptop)

**Hardware:** Intel Celeron 900 (Released Q1 2009: 1 Core, 1 Thread @ 2.19 GHz, 1MB L2 Cache, ~4GB RAM)
**Environment:** Ubuntu Sway Wayland (Plucky Puffin), Python 3.10

This test provides an extreme stress test of the runtime's single-thread contention, atomic locking, and cache locality.

### Results

| Test Type | Target Actors | Spawn Rate | Message Throughput | Time to Send |
| --- | --- | --- | --- | --- |
| **Pull (Threaded)** | 100,000 | ~36,672 actors/sec | **~473,851 msgs/sec** | 0.21s |
| **Push (Green)** | 100,000 | ~36,503 actors/sec | **~385,141 msgs/sec** | 0.26s |

#### Analysis: Mechanical Sympathy & Cache Locality

While the single-core allocation speed (Spawn Rate) drops to ~36,000 actors/sec due to the lack of concurrent threads processing the allocations, **the message throughput actually beats the modern 8-core ARM processor.**

This is a textbook demonstration of **mechanical sympathy**:

1. **Zero Lock Contention:** Because the Celeron only has one core, Tokio's multithreaded runtime collapses into a single-threaded run queue. The sender and receiver always execute on the exact same core, completely eliminating cross-core atomic lock contention on the `mpsc` channels.
2. **L2 Cache Warmth:** The 1MB L2 cache stays perfectly warm. Instead of syncing memory boundaries across 8 different cores (false sharing), the single CPU just swaps pointers inside its local cache, allowing a 15-year-old processor to push nearly half a million messages per second.

<details>
<summary><strong>View Raw Output (Pull / 100k)</strong></summary>

```text
--- Myrmidon Mailbox Stress Test: 0.1.3 ---
Goal: Spawn 100000 threaded mailbox actors.
🚀 Spawning (Threaded)...
   [10000/100000] Spawned. Rate: 28999 actors/sec
   ...
✅ Spawned 100000 actors in 2.73s (36672 actors/sec)
📨 Sending 100000 messages...
✅ Sent 100000 messages in 0.21s (473851 msgs/sec)
🛑 Cleaning up (stopping all actors)...
Done.

```

</details>

---

## Conclusion

Myrmidon's core loop—allocating PIDs, routing bytes through channels, and executing Tokio tasks—is purely CPU-bound and instruction-cache friendly.

By achieving upwards of **470,000 messages per second** and juggling **100,000 concurrent threaded actors** on heavily constrained and legacy hardware, the runtime proves it carries virtually zero structural bloat. On modern, multi-core server infrastructure (e.g., AMD EPYC, Intel Xeon, AWS Graviton), Myrmidon's scalability will be bound almost entirely by network bandwidth rather than CPU overhead.
