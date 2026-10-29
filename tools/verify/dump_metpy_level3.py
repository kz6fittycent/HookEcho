#!/usr/bin/env python3
"""Reference dump of a Level III radial product via MetPy, for hookecho-core's
tests (crates/hookecho-core/tests/level3_vs_metpy.rs).

Records the product code, radar position, volume time, elevation, the data
level scaling (MetPy's value for codes 2 and 255), the radial count, gate
count, radial start/delta angles sorted by start azimuth, and the SHA-256 of
the raw 8-bit codes, radial-major in that sorted order (hookecho sorts
radials by azimuth so a scan's products line up).

The JSON is committed next to the sample, so CI needs no Python.

Usage: dump_metpy_level3.py FILE > FILE.metpy.json
Requires: metpy (BSD-3), numpy (BSD-3). Not shipped with the app.
"""
import hashlib
import json
import sys

import numpy as np
from metpy.io import Level3File

path = sys.argv[1]
f = Level3File(path)
packet = f.sym_block[0][0]
starts = [float(a) for a in packet["start_az"]]
deltas = [float(e - s) % 360.0 for s, e in zip(packet["start_az"], packet["end_az"])]
order = sorted(range(len(starts)), key=lambda i: starts[i])
gates = len(packet["data"][0])
raw = np.concatenate([np.asarray(packet["data"][i], dtype=np.uint8)[:gates] for i in order])
mapped = f.map_data([2, 255])

out = {
    "file": path.split("/")[-1],
    "metpy": __import__("metpy").__version__,
    "code": int(f.header.code),
    "lat": round(float(f.lat), 4),
    "lon": round(float(f.lon), 4),
    "height_ft": int(f.height),
    "volume_time": f.metadata["vol_time"].isoformat() + "Z",
    "elevation_deg": round(float(f.metadata["el_angle"]), 2),
    "value_at_2": round(float(mapped[0]), 3),
    "value_at_255": round(float(mapped[1]), 3),
    "radials": len(order),
    "gates": gates,
    "start_az": [round(starts[i], 2) for i in order],
    "delta_az": [round(deltas[i], 2) for i in order],
    "raw_sha256": hashlib.sha256(raw.tobytes()).hexdigest(),
}
json.dump(out, sys.stdout, indent=1)
print()
