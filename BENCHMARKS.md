# Iris Benchmarks

The following benchmarks demonstrate Iris's raw throughput and memory efficiency. To test the extreme boundaries of the runtime's memory footprint, synchronization overhead, and cache locality, these benchmarks were run on both **severely constrained hardware** and **shared cloud virtual environments**.

If Iris can juggle 100,000 concurrent actors on a phone and a 15-year-old laptop, it flies on modern server architecture.

## Methodology

We test the two primary actor paradigms supported by Iris:
1. **Push Actors (Green Threads):** Pure Tokio futures. The Rust runtime pushes messages directly to the callbacks. Highly efficient, zero-cost abstractions.
2. **Pull Actors (Threaded / OS Threads):** Standard "Erlang-style" blocking actors. The runtime provisions a dedicated OS thread (via Tokio's blocking pool) where the interpreter (e.g., Python) waits on a `recv()` call, releasing the GIL.

**The Goal:** Spawn up to 100,000 actors, blitz them with messages, and perform a clean teardown.

---

## Test Environment 1: The Modern Edge (Android Smartphone)

**Hardware:** Infinix X669 (Unisoc T606 - 8 Cores @ 1.61 GHz, 4GB RAM)
**Environment:** Termux, Ubuntu PRoot, Python 3.10

This test validates Iris's ability to allocate and schedule massive concurrency on low-power ARM architecture.

### Results

| Test Type | Target Actors | Spawn Rate | Message Throughput | Time to Send |
| :--- | :--- | :--- | :--- | :--- |
| **Pull (Threaded)** | 50,000 | ~113,841 actors/sec | **~391,111 msgs/sec** | 0.13s |
| **Push (Green)** | 100,000 | ~96,812 actors/sec | **~337,913 msgs/sec** | 0.30s |

#### Analysis: Memory and Allocation
Spawning 50,000 dedicated OS-thread backed actors on a mobile device without triggering the Linux OOM (Out-Of-Memory) killer validates the extreme leanness of the `Mailbox` architecture. The spawn rate on the 8-core ARM chip scales beautifully, easily breaking 100k allocations per second. 

---

## Test Environment 2: The Legacy Constraint (15-Year-Old Laptop)

**Hardware:** Intel Celeron 900 (Released Q1 2009: 1 Core, 1 Thread @ 2.19 GHz, 1MB L2 Cache, ~4GB RAM)
**Environment:** Ubuntu Sway Wayland (Plucky Puffin), Python 3.10, Cold Ambient Temperature

This test provides an extreme stress test of the runtime's single-thread contention, atomic locking, and cache locality, deliberately run in a thermally forgiving environment to prevent CPU clock throttling.

### Results

| Test Type | Target Actors | Spawn Rate | Message Throughput | Time to Send |
| --- | --- | --- | --- | --- |
| **Pull (Threaded)** | 100,000 | ~41,443 actors/sec | **~563,258 msgs/sec** | 0.18s |
| **Push (Green)** | 100,000 | ~42,431 actors/sec | **~409,927 msgs/sec** | 0.24s |

**Hot Swap Performance:** ~85,119 logic swaps/sec while sustaining ~340,477 msgs/sec.

#### Analysis: Mechanical Sympathy & Cache Locality

While the single-core allocation speed is naturally lower than modern multi-core chips, **the message throughput aggressively beats the modern 8-core ARM processor.**

This is a textbook demonstration of **mechanical sympathy**:

1. **Zero Lock Contention:** Because the Celeron only has one core, Tokio's multithreaded runtime collapses into a single-threaded run queue. The sender and receiver always execute on the exact same core, completely eliminating cross-core atomic lock contention on the `mpsc` channels.
2. **L2 Cache Warmth:** The 1MB L2 cache stays perfectly warm. Instead of syncing memory boundaries across 8 different cores (false sharing), the single CPU just swaps pointers inside its local cache, allowing a 15-year-old processor to push over half a million messages per second.
3. **Thermal Headroom:** By running in a cold environment, the 65nm legacy silicon was prevented from thermal throttling, allowing it to sustain its peak 2.19 GHz clock across the entire benchmark.

---

## Test Environment 3: Cloud Virtualization (vCPUs)

**Hardware:** Shared Enterprise Silicon (2 vCPUs allocated via Hypervisor)
**Environment:** GitHub Codespaces (AMD EPYC 7763) & Google Colab (Intel Xeon @ 2.20GHz), Ubuntu 22.04 LTS

These tests validate how Iris performs in "noisy neighbor" virtualized environments typical of modern CI/CD pipelines and cloud deployments.

### Results (GitHub Codespaces - AMD EPYC 7763)

| Test Type | Target Actors | Spawn Rate | Message Throughput |
| --- | --- | --- | --- |
| **Pull (Threaded)** | 100,000 | ~182,040 actors/sec | **~1,597,836 msgs/sec** |
| **Push (Green)** | 100,000 | ~182,536 actors/sec | **~1,200,242 msgs/sec** |
| **Hot Swap Blitz** | 5,000 swaps | **~91,632 swaps/sec** | ~366,528 msgs/sec |

### Results (Google Colab - Intel Xeon)

| Test Type | Target Actors | Spawn Rate | Message Throughput |
| --- | --- | --- | --- |
| **Pull (Threaded)** | 100,000 | ~136,273 actors/sec | **~803,230 msgs/sec** |
| **Push (Green)** | 100,000 | ~131,072 actors/sec | **~794,230 msgs/sec** |
| **Hot Swap Blitz** | 5,000 swaps | **~136,428 swaps/sec** | ~545,711 msgs/sec |

#### Analysis: The 1.5 Million Barrier & Monolithic vs Chiplet Silicon

Despite being heavily virtualized and time-sliced by hypervisors, Iris shatters the **1.5 million messages per second** barrier on the AMD EPYC instance. This demonstrates that Tokio's underlying channel implementations and Iris's Rust-to-Python FFI boundary scale exceptionally well with modern IPC (Instructions Per Clock).

Interestingly, while the AMD EPYC crushed the Intel Xeon in raw message passing (1.59M vs 803k), the Intel Xeon won the hot-swap benchmark (136k vs 91k). Atomic operations (such as the pointer swapping used in Iris's live hot-swap functionality) often resolve faster on older monolithic silicon architectures compared to chiplet-based designs like AMD's Zen architecture, where cache coherency must sync across the Infinity Fabric.

---

## Conclusion

Iris's core loop—allocating PIDs, routing bytes through channels, and executing Tokio tasks—is purely CPU-bound and instruction-cache friendly.

By achieving upwards of **1.59 million messages per second** on shared cloud vCPUs, sustaining **136,000 logic hot-swaps per second**, and juggling **100,000 concurrent threaded actors** on heavily constrained and legacy hardware, the runtime proves it carries virtually zero structural bloat. Whether running on a 15-year-old laptop or an enterprise AMD EPYC server rack, Iris delivers BEAM-class, Erlang-like concurrency to modern scripting languages.
