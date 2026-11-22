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
    // input neurons
    (nid=input-1, grp=group1, nty=mi, mst=1 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=2 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=1 mm, px2=0 µm, px3=0 µm)),
    (nid=input-2, grp=group1, nty=mi, mst=1 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=2 mV, csp=0.5 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-1 mm, px2=0 µm, px3=0 µm)),

    // output neuron
    (nid=output-1, grp=group1, nty=mi, mst=1 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=10 mm, px3=0 µm))
],

CON=[
    // input to output
    (prn=input-1, psn=output-1, cnw=1.0, eqw=1.0, lrn=stdp),
    (prn=input-2, psn=output-1, cnw=1.0, eqw=1.0, lrn=stdp)
],

LRN=[
    (fnc=stdp, ina=0.035, inp=40 ms, exa=0.015, exp=20 ms),
    (fnc=flat)
]
)