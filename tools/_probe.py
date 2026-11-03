import os, re

def rv(buf, i):
    r = 0; s = 0
    while True:
        x = buf[i]; i += 1
        r |= (x & 0x7f) << s
        if not (x & 0x80):
            return r, i
        s += 7

def parse(buf, i=0, end=None):
    if end is None:
        end = len(buf)
    out = {}
    while i < end:
        try:
            key, i = rv(buf, i)
        except Exception:
            break
        f, wt = key >> 3, key & 7
        if f == 0 or f > 20:
            break
        if wt == 0:
            try:
                v, i = rv(buf, i)
            except Exception:
                break
        elif wt == 2:
            try:
                ln, i = rv(buf, i)
            except Exception:
                break
            if i + ln > end:
                break
            v = buf[i:i + ln]; i += ln
        elif wt == 5:
            v = buf[i:i + 4]; i += 4
        elif wt == 1:
            v = buf[i:i + 8]; i += 8
        else:
            break
        out.setdefault(f, []).append(v)
    return out, i

def txt(v):
    return v.decode("utf-8", "replace")

buf = open(os.environ["PB_BIN"], "rb").read()
names = sorted(set(re.findall(rb"[a-zA-Z0-9_/.-]{3,80}\.proto", buf)))
found = {}
for nm in names:
    if not nm.endswith(b".proto") or b"google" in nm:
        continue
    ln = len(nm)
    if ln < 128:
        pat = bytes([0x0a, ln]) + nm
    else:
        pat = bytes([0x0a, (ln & 0x7f) | 0x80, ln >> 7]) + nm
    s = 0
    while True:
        i = buf.find(pat, s)
        if i < 0:
            break
        fd, _ = parse(buf, i, i + 8192)
        if 1 in fd and fd[1] and txt(fd[1][0]) == txt(nm):
            found[txt(nm)] = fd
            break
        s = i + 1

print("descriptors:", len(found))
for name, fd in sorted(found.items()):
    svcs = []
    for sv in fd.get(6, []):
        sd, _ = parse(sv)
        if 1 in sd:
            svcs.append(txt(sd[1][0]))
    if svcs:
        print(name, "->", svcs)
