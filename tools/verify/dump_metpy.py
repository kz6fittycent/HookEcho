#!/usr/bin/env python3
"""Reference dump of Level II sweeps via MetPy, for hookecho-core's tests.

For every sweep and moment it records the radial azimuths and elevations, the
gate geometry, scale/offset, and the SHA-256 of the *raw* gate codes (u16,
little-endian, radial-major in file order, each radial padded to the sweep's
max gate count with 0). MetPy returns scaled floats with codes 0/1 masked, so
the raw codes are rebuilt as round(value * scale + offset). MetPy maps both
code 0 (below threshold) and code 1 (range folded) to NaN, so RANGE_FOLD is
overridden with a sentinel to keep the two apart.

The JSON is committed next to the sample, so CI needs no Python.

Usage: dump_metpy.py FILE.ar2[.gz] > FILE.metpy.json
Requires: metpy (BSD-3), numpy (BSD-3). Not shipped with the app.
"""
import gzip
import hashlib
import io
import json
import sys

import numpy as np
from metpy.io import Level2File

path = sys.argv[1]
data = open(path, "rb").read()
if data[:2] == b"\x1f\x8b":
    data = gzip.decompress(data)
RF = -1.0e30
Level2File.RANGE_FOLD = RF
f = Level2File(io.BytesIO(data))

out = {"file": path.split("/")[-1], "metpy": __import__("metpy").__version__, "sweeps": []}
for s in f.sweeps:
    sweep = {
        "radials": len(s),
        "azimuth": [round(float(r[0].az_angle), 4) for r in s],
        "elevation": [round(float(r[0].el_angle), 4) for r in s],
        "moments": {},
    }
    names = sorted({m for r in s for m in r[4].keys()})
    for name in names:
        hdrs = [r[4][name][0] for r in s if name in r[4]]
        h = hdrs[0]
        gates = max(x.num_gates for x in hdrs)
        rows = []
        for r in s:
            row = np.zeros(gates, dtype="<u2")
            if name in r[4]:
                hdr, vals = r[4][name]
                vals = np.asarray(vals, dtype=np.float64)
                codes = np.where(np.isnan(vals), 0, np.where(vals == RF, 1, np.round(vals * hdr.scale + hdr.offset)))
                row[: len(codes)] = codes.astype("<u2")
            rows.append(row)
        arr = np.stack(rows)
        sweep["moments"][name.decode()] = {
            "gates": int(gates),
            "first_gate_km": float(h.first_gate),
            "gate_km": float(h.gate_width),
            "word_size": int(h.data_size),
            "scale": float(h.scale),
            "offset": float(h.offset),
            "sha256": hashlib.sha256(arr.tobytes()).hexdigest(),
            # A few raw codes at fixed positions, to make mismatches easy to read.
            "spot": [[int(i), int(j), int(arr[i, j])] for i, j in ((0, 0), (len(s) // 2, gates // 3), (len(s) - 1, gates // 2))],
        }
    out["sweeps"].append(sweep)
json.dump(out, sys.stdout, indent=1)
