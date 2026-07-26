import json, difflib, sys
sys.path.insert(0, ".")
from streaming_bench import normalise
ref = normalise(open("fixtures/watchshop_60s_reference.txt", encoding="utf-8").read())
hyp = normalise(json.load(open("results_stream.json"))[0]["transcript"])
sm = difflib.SequenceMatcher(None, ref, hyp, autojunk=False)
for tag, i1, i2, j1, j2 in sm.get_opcodes():
    if tag == "equal":
        continue
    r = " ".join(ref[i1:i2])
    h = " ".join(hyp[j1:j2])
    print("%-9s ref=%-34s hyp=%s" % (tag, repr(r), repr(h)))
