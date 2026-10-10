#!/usr/bin/env python3
"""Trim a NEXRAD Archive II volume to its first N complete sweeps.

Every kept byte is identical to the original, so tests run on real data while
staying small. Two on-disk layouts exist:

* LDM-compressed (2008+ real-time era, all files on AWS since ~2016): a 24-byte
  volume header, then records of [4-byte big-endian signed length][bzip2 data].
  We keep whole records, so the cut lands on the first record boundary at or
  after the end of the Nth sweep.
* Uncompressed (older archive files, often gzip-wrapped): the 24-byte header,
  then messages back to back, each with a 12-byte CTM prefix. Message 31 is
  variable length (its size field, in halfwords); other messages occupy fixed
  2432-byte segments.

Sweep ends are found from the Message 31 radial status (4 = end of volume,
2 = end of elevation).

Format references: NOAA ROC ICD 2620010 (Archive II/User) for the file and
record layout; ICD 2620002 for message headers and the Message 31 data header.
Field offsets used here are noted inline and were cross-checked against MetPy
(metpy/io/nexrad.py) on the sample files.

Usage: trim_level2.py SRC DST SWEEPS   (SRC may be gzip-wrapped)
"""
import bz2
import gzip
import struct
import sys

CTM = 12          # CTM prefix before every message header
MSG_HDR = 16      # message header: size (u16 halfwords), channel, type, ...
SEGMENT = 2432    # fixed segment size for non-31 messages
# Offset of "radial status" inside the Message 31 data header:
# ICAO(4) time(4) date(2) azimuth number(2) azimuth(4) compression(1)
# spare(1) radial length(2) azimuth resolution(1) -> 21.
RADIAL_STATUS = 21
END_ELEVATION, END_VOLUME = 2, 4


def messages(buf):
    """Yield (offset_after_message, type, radial_status) for each message."""
    i = 0
    while i + CTM + MSG_HDR <= len(buf):
        size_halfwords, mtype = struct.unpack(">HxB", buf[i + CTM:i + CTM + 4])
        if mtype == 31:
            end = i + CTM + size_halfwords * 2
            status = buf[i + CTM + MSG_HDR + RADIAL_STATUS]
        else:
            end = i + SEGMENT
            status = None
        if size_halfwords == 0 and mtype == 0:
            end = i + SEGMENT  # padding segment
        yield end, mtype, status
        i = end


def main():
    src, dst, want = sys.argv[1], sys.argv[2], int(sys.argv[3])
    raw = open(src, "rb").read()
    if raw[:2] == b"\x1f\x8b":
        raw = gzip.decompress(raw)
    out = bytearray(raw[:24])
    sweeps = 0
    compressed = raw[24:28] != b"\x00\x00\x00\x00" and raw[28:31] == b"BZh"
    if compressed:
        i = 24
        while i < len(raw) and sweeps < want:
            (size,) = struct.unpack(">i", raw[i:i + 4])
            size = abs(size)
            record = raw[i:i + 4 + size]
            out += record
            i += 4 + size
            for _, mtype, status in messages(bz2.decompress(record[4:])):
                if mtype == 31 and status in (END_ELEVATION, END_VOLUME):
                    sweeps += 1
    else:
        body = raw[24:]
        for end, mtype, status in messages(body):
            if mtype == 31 and status in (END_ELEVATION, END_VOLUME):
                sweeps += 1
                if sweeps == want:
                    out += body[:end]
                    break
    if sweeps < want:
        sys.exit(f"only {sweeps} sweeps in {src}")
    open(dst, "wb").write(out)
    print(f"{dst}: {len(out)} bytes, {sweeps} sweeps ({'LDM bzip2' if compressed else 'uncompressed'})")


if __name__ == "__main__":
    main()
