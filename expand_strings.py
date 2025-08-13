def windows(l, size):
    w = []

    for i in range(len(l) - size + 1):
        w.append(l[i:i+size])

    return w

f = open("wordlist.txt", 'r', encoding='utf-8', errors='replace')

CHARSET = "0123456789abcdefghijklmnopqrstuvwxyz_-. "
strings = set()
for line in f.readlines():
    line = line.strip()
    if len(line) == 0:
        continue
    strings.add(line)
    if "_" not in line:
        continue
    # print(line.encode())
    # Collapse double underscores
    while "__" in line:
        line = line.replace("__", "_")

    parts = [x for x in line.split("_") if len(x) > 1]
    if len(parts) == 1:
        continue
    for i in range(2, len(parts)):
        for s in windows(parts, i):
            strings.add("_".join(s))


    split_part = ""
    for c in line.lower():
        if c not in CHARSET:
            if len(split_part) > 1:
                strings.add(split_part)
                split_part = ""
            continue
        split_part += c

    if len(split_part) != len(line) and len(split_part) > 1:
        strings.add(split_part)

out = open("wordlist_expanded.txt", "w", encoding='utf-8', errors='replace')
for line in sorted(strings):
    out.write(line + "\n")
out.close()

out = open("wordlist_expanded_purified.txt", "w", encoding='utf-8', errors='replace')
purified = set()
for line in sorted(strings):
    line = line.lower()
    if all(c in CHARSET for c in line):
        # print("Pure line:", line)
        purified.add(line)
        # out.write(line + "\n")
    # else:
    #     print("Impure line:", line)

for line in sorted(purified):
    out.write(line + "\n")

out.close()
