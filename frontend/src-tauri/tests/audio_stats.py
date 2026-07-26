import array, math, sys
def stats(path, name):
    a = array.array("f"); d = open(path,"rb").read(); a.frombytes(d[:len(d)-len(d)%4])
    n = len(a)
    peak = max(abs(x) for x in a)
    rms = math.sqrt(sum(x*x for x in a)/n)
    clipped = sum(1 for x in a if abs(x) >= 0.999)
    quiet = sum(1 for i in range(0,n,160) if max(abs(y) for y in a[i:i+160] or [0]) < 0.01)
    print("%-16s peak %.3f  rms %.4f (%.1f dBFS)  clipped %d (%.3f%%)  quiet-10ms %.0f%%"
          % (name, peak, rms, 20*math.log10(rms+1e-12), clipped, 100*clipped/n, 100*quiet/(n/160)))
stats("fixtures/watchshop_60s_16k.f32", "clean fixture")
stats("fixtures/device_60s_16k.f32", "device capture")
