// basic oscillator
(
/*
    neurons
*/
NRN=[
    // input layer
    (nid=input-1, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0, px2=0, px3=0)),
    (nid=input-2, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=2, px2=0, px3=0)),
    (nid=input-3, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=4, px2=0, px3=0)),
    (nid=input-4, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=6, px2=0, px3=0)),
    (nid=input-5, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=8, px2=0, px3=0)),
    (nid=input-6, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10, px2=0, px3=0)),
    (nid=input-7, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=12, px2=0, px3=0)),
    (nid=input-8, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=14, px2=0, px3=0)),
    (nid=input-9, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=16, px2=0, px3=0)),
    (nid=input-10, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=18, px2=0, px3=0)),

    // hidden layer (excitation neurons)
    (nid=hidden-1, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0, px2=20, px3=0)),
    (nid=hidden-2, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=2, px2=20, px3=0)),
    (nid=hidden-3, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=4, px2=20, px3=0)),
    (nid=hidden-4, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=6, px2=20, px3=0)),
    (nid=hidden-5, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=8, px2=20, px3=0)),
    (nid=hidden-6, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10, px2=20, px3=0)),
    (nid=hidden-7, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=12, px2=20, px3=0)),
    (nid=hidden-8, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=14, px2=20, px3=0)),
    (nid=hidden-9, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=16, px2=20, px3=0)),
    (nid=hidden-10, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=18, px2=20, px3=0)),

    // hidden layer (inhibition neurons)
    (nid=hidden-11, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0, px2=20, px3=2)),
    (nid=hidden-12, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=2, px2=20, px3=2)),
    (nid=hidden-13, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=4, px2=20, px3=2)),
    (nid=hidden-14, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=6, px2=20, px3=2)),
    (nid=hidden-15, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=8, px2=20, px3=2)),
    (nid=hidden-16, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10, px2=20, px3=2)),
    (nid=hidden-17, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=12, px2=20, px3=2)),
    (nid=hidden-18, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=14, px2=20, px3=2)),
    (nid=hidden-19, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=16, px2=20, px3=2)),
    (nid=hidden-20, nty=mi, mst=5, inh=t, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=18, px2=20, px3=2)),

    // output layer
    (nid=output-1, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0, px2=40, px3=0)),
    (nid=output-2, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=2, px2=40, px3=0)),
    (nid=output-3, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=4, px2=40, px3=0)),
    (nid=output-4, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=6, px2=40, px3=0)),
    (nid=output-5, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=8, px2=40, px3=0)),
    (nid=output-6, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10, px2=40, px3=0)),
    (nid=output-7, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=12, px2=40, px3=0)),
    (nid=output-8, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=14, px2=40, px3=0)),
    (nid=output-9, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=16, px2=40, px3=0)),
    (nid=output-10, nty=mi, mst=5, inh=f, rfp=20.0, mnp=0.0, mpd=2500, spp=1.0, csp=1, WDF=(fnc=exp, dhl=100000), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=18, px2=40, px3=0))
],

/*
    connections
*/
CON=[
    // left side
    (prn=input-{1:2:1,3,4,5}, psn=hidden-{1:5:1}, cnw=1.5, lrn=stdp),
    (prn=input-{1:5:1}, psn=hidden-{11:15:1}, cnw=1.5, lrn=stdp),

    // right side
    (prn=input-{6:10:1}, psn=hidden-{6:10:1}, cnw=1.5, lrn=stdp),
    (prn=input-{6:10:1}, psn=hidden-{16:20:1}, cnw=1.5, lrn=stdp),

    // cross-over inhibition
    (prn=hidden-{11:15:1}, psn=input-{1:5:1}, cnw=1.5, lrn=stdp),
    (prn=hidden-{16:20:1}, psn=input-{6:10:1}, cnw=1.5, lrn=stdp),

    // cross-over excitation
    (prn=hidden-{1:5:1}, psn=input-{6:10:1}, cnw=1.5, lrn=stdp),
    (prn=hidden-{6:10:1}, psn=input-{1:5:1}, cnw=1.5, lrn=stdp)
],

/*
    learning functions (spike-timing-dependent plasticity)
*/
LRN=[
    (fnc=stdp, ina=0.035, inp=3, exa=0.035, exp=3),
    (fnc=flat)
],

 CST=[
     (fnc=sig, dhl=10000),
     (fnc=none)
 ]
)