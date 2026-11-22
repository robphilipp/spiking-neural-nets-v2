// line sensor network
// For parameters that accept units, if they are not specified, they default to:
// • distances to µm
// • times to ms
// • conductance speeds to m/s
// • electric potentials to mV
// • frequencies to Hz
// • magnetic flux to Wb
(
GRP=[
    (gid=group1)
],
NRN=[
    // OUTPUT LAYER
    (nid=output-1, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=20 µm)),
    (nid=output-2, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=20 µm)),
    (nid=output-3, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=20 µm)),
    (nid=output-4, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=20 µm)),
    (nid=output-5, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=20 µm)),
    (nid=output-6, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=20 µm)),
    (nid=output-7, grp=group1, nty=mi, mst=6.5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=25 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=20 µm))
],

CON=[
],

LRN=[
    (fnc=stdp, ina=0.035, inp=4 s, exa=0.015, exp=2 s),
    (fnc=flat)
],

CST=[
     (fnc=sig, dhl=10 s),
     (fnc=none)
]
)