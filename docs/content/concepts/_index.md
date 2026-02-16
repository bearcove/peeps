+++
title = "Concepts"
weight = 1
sort_by = "weight"
insert_anchor_links = "heading"
+++

peeps is a graph-first causality debugger for async Rust. It models your program as a graph of **nodes** (tasks, resources, requests) connected by **edges** (causal relationships). Understanding this graph is the key to understanding what your program is doing and why it's stuck.

This section covers the mental model: what nodes and edges represent, how they're created and removed, how the causality stack ties them together, and what timing information is available.
