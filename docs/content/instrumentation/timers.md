+++
title = "Timers"
weight = 4
+++

Three timer wrappers:

## sleep

`peeps::sleep(duration)` wraps `tokio::time::sleep`. Creates a `Sleep` node. Tracks `elapsed_ns`.

## interval

`peeps::interval(period)` and `peeps::interval_at(start, period)` wrap `tokio::time::interval`. Creates an `Interval` node. Tracks tick count and missed tick behavior.

## timeout

`peeps::timeout(duration, future)` wraps `tokio::time::timeout`. Creates a `Timeout` node. Tracks `elapsed_ns`.

All timer nodes exist for the duration of the timer and are removed when it completes or is dropped.
