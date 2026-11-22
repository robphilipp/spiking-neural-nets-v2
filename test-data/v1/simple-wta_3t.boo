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
    // input layer
    (nid=input-1, grp=group1, nty=mi, mst=1 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=0.5 mV, csp=0.1 m/s,
        WDF=(fnc=zer),
        SRP=(fcb=1000, fcm=0.1, fct=100 ms, dpb=1000, dpm=0.1, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=0 µm, px2=0, px3=100 µm)
    ),
    (nid=input-2, grp=group1, nty=mi, mst=1 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=0.5 mV, csp=0.1 m/s,
        WDF=(fnc=zer),
        SRP=(fcb=1000, fcm=0.1, fct=100 ms, dpb=1000, dpm=0.1, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=0 µm, px2=0, px3=100 µm)
    ),

    // inhibition neuron
    (nid=inhib-1, grp=group1, nty=mi, mst=1.0 mV, inh=t, rfp=0.1 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=2 mV, csp=0.08 m/s,
        WDF=(fnc=exp, dhl=2 h),
        SRP=(fcb=1000, fcm=0, fct=100 ms, dpb=1000, dpm=0, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=0 µm, px2=0, px3=0 µm)
    ),
    (nid=inhib-2, grp=group1, nty=mi, mst=1.0 mV, inh=t, rfp=0.1 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=2 mV, csp=0.08 m/s,
        WDF=(fnc=exp, dhl=2 h),
        SRP=(fcb=1000, fcm=0, fct=100 ms, dpb=1000, dpm=0, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=0 µm, px2=0, px3=0 µm)
    ),
    (nid=inhib-3, grp=group1, nty=mi, mst=1.0 mV, inh=t, rfp=0.1 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=2 mV, csp=0.08 m/s,
        WDF=(fnc=exp, dhl=2 h),
        SRP=(fcb=1000, fcm=0, fct=100 ms, dpb=1000, dpm=0, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=0 µm, px2=0, px3=0 µm)
    ),

    // output layer
    (nid=output-1, grp=group1, nty=mi, mst=2 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s,
        WDF=(fnc=exp, dhl=2 h),
        SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=300 µm, px2=0, px3=0 µm)
    ),
    (nid=output-2, grp=group1, nty=mi, mst=2 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s,
        WDF=(fnc=exp, dhl=2 h),
        SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=300 µm, px2=2.0944, px3=0 µm)
    ),
    (nid=output-3, grp=group1, nty=mi, mst=2 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s,
        WDF=(fnc=exp, dhl=2 h),
        SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms),
        WLF=(fnc=bnd, lwb=0.0, upb=1.0),
        LOC=(cst=cl, px1=300 µm, px2=-2.0944, px3=0 µm)
    )
],

CON=[
    // input to output
    (prn=input-1, psn=output-1, cnw=r0.1:0.9, eqw=r0.1:0.9, lrn=stdp),
    (prn=input-2, psn=output-1, cnw=r0.1:0.9, eqw=r0.1:0.9, lrn=stdp),
    (prn=input-1, psn=output-2, cnw=r0.1:0.9, eqw=r0.1:0.9, lrn=stdp),
    (prn=input-2, psn=output-2, cnw=r0.1:0.9, eqw=r0.1:0.9, lrn=stdp),
    (prn=input-1, psn=output-3, cnw=r0.1:0.9, eqw=r0.1:0.9, lrn=stdp),
    (prn=input-2, psn=output-3, cnw=r0.1:0.9, eqw=r0.1:0.9, lrn=stdp),

    // output to inhibition
    (prn=output-1, psn=inhib-1, cnw=2, eqw=2, lrn=flat),
    (prn=output-2, psn=inhib-2, cnw=2, eqw=2, lrn=flat),
    (prn=output-3, psn=inhib-3, cnw=2, eqw=2, lrn=flat),

    // inhib to output
    (prn=inhib-1, psn=output-{2,3}, cnw=1, eqw=1, lrn=flat),
    (prn=inhib-2, psn=output-{1,3}, cnw=1, eqw=1, lrn=flat),
    (prn=inhib-3, psn=output-{1,2}, cnw=1, eqw=1, lrn=flat)
],

LRN=[
    (fnc=stdp, ina=0.030, inp=30 ms, exa=0.015, exp=20 ms),
    //(fnc=stdp, ina=0.020, inp=30 ms, exa=0.010, exp=20 ms),
    //(fnc=stdp, ina=0.040, inp=30 ms, exa=0.020, exp=20 ms),
    //(fnc=stdp, ina=0.080, inp=30 ms, exa=0.040, exp=20 ms),
    //(fnc=stdp, ina=0.040, inp=30 ms, exa=0.020, exp=20 ms),
    (fnc=flat)
]
)