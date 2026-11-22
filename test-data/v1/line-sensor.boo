// line sensor network
// For parameters that accept units, if they are not specified, they default to:
// • distances to µm
// • times to ms
// • conductance speeds to m/s
// • electric potentials to mV
// • frequencies to Hz
(
GRP=[
    (gid=group3),
//    (gid=group3, hst=192.168.1.174, prt=2552),
//    (gid=group3, hst=192.168.1.177, prt=2553),
    (gid=group1),
//    (gid=group1, hst=192.168.1.177, prt=2553),
//    (gid=group1, hst=192.168.1.153, prt=2552),
//    (gid=group1, hst=192.168.1.174, prt=2552),
//    (gid=group4, hst=192.168.1.153, prt=2553),
    (gid=group2, hst=192.168.1.175, prt=2552)
],
NRN=[
    // SENSOR ARRAY
    // column 1
    (nid=input-11, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30, px2=-30, px3=0.)),
    (nid=input-12, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-13, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-14, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=0 µm)),
    (nid=input-15, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=10 µm, px3=0 µm)),
    (nid=input-16, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=20 µm, px3=0 µm)),
    (nid=input-17, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=30 µm, px3=0 µm)),

    // column 2
    (nid=input-21, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-22, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-23, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-24, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=0 µm)),
    (nid=input-25, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=10 µm, px3=0 µm)),
    (nid=input-26, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=20 µm, px3=0 µm)),
    (nid=input-27, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=30 µm, px3=0 µm)),

    // column 3
    (nid=input-31, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-32, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-33, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-34, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=0 µm)),
    (nid=input-35, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=10 µm, px3=0 µm)),
    (nid=input-36, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=20 µm, px3=0 µm)),
    (nid=input-37, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=30 µm, px3=0 µm)),

    // column 4
    (nid=input-41, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-42, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-43, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-44, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=0 µm)),
    (nid=input-45, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=10 µm, px3=0 µm)),
    (nid=input-46, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=20 µm, px3=0 µm)),
    (nid=input-47, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=30 µm, px3=0 µm)),

    // column 5
    (nid=input-51, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-52, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-53, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-54, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=0 µm)),
    (nid=input-55, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=10 µm, px3=0 µm)),
    (nid=input-56, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=20 µm, px3=0 µm)),
    (nid=input-57, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=30 µm, px3=0 µm)),

    // column 6
    (nid=input-61, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-62, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-63, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-64, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=0 µm)),
    (nid=input-65, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=10 µm, px3=0 µm)),
    (nid=input-66, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=20 µm, px3=0 µm)),
    (nid=input-67, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=30 µm, px3=0 µm)),

    // column 7
    (nid=input-71, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-72, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-73, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-74, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=0 µm)),
    (nid=input-75, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=10 µm, px3=0 µm)),
    (nid=input-76, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=20 µm, px3=0 µm)),
    (nid=input-77, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=250 ms, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=30 µm, px3=0 µm)),

    // INHIBITION LAYER
    // (nid=inhib-1, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=5 µm)),
    // (nid=inhib-2, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=5 µm)),
    // (nid=inhib-3, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=5 µm)),
    // (nid=inhib-4, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=5 µm)),
    // (nid=inhib-5, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=5 µm)),
    // (nid=inhib-6, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=5 µm)),
    // (nid=inhib-7, grp=group1, nty=mi, mst=5 mV, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=10 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-1, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-2, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-3, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-4, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-5, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-6, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=5 µm)),
    (nid=inhib-7, grp=group3, nty=bi, lct=5 mV, rst=4.5 mV, tfr=100 Hz, inh=t, rfp=20 ms, mnp=0 mV, mpd=25 ms, spp=0.5 mV, csp=1 m/s, WDF=(fnc=exp, dhl=100 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=5 µm)),

    // PROCESSING LAYER
    (nid=hidden-1, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=50 µm)),
    (nid=hidden-2, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=50 µm)),
    (nid=hidden-3, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=50 µm)),
    (nid=hidden-4, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=50 µm)),
    (nid=hidden-5, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=50 µm)),
    (nid=hidden-6, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=50 µm)),
    (nid=hidden-7, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=2 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=50 µm)),

    // OUTPUT LAYER
    (nid=output-1, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=20 µm)),
    (nid=output-2, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=20 µm)),
    (nid=output-3, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=20 µm)),
    (nid=output-4, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=20 µm)),
    (nid=output-5, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=20 µm)),
    (nid=output-6, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=20 µm)),
    (nid=output-7, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, mnp=0 mV, mpd=2.5 s, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=1 s), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=20 µm))
],

CON=[
    // input to processing
    (prn=input-1{1:7:1}, psn=hidden-1, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-2{1:7:1}, psn=hidden-2, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-3{1:7:1}, psn=hidden-3, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-4{1:7:1}, psn=hidden-4, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-5{1:7:1}, psn=hidden-5, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-6{1:7:1}, psn=hidden-6, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-7{1:7:1}, psn=hidden-7, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),

    // input to inhibition
    (prn=input-1{1:7:1}, psn=inhib-{2,3,4,5,6,7}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-2{1:7:1}, psn=inhib-{1,3,4,5,6,7}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-3{1:7:1}, psn=inhib-{1,2,4,5,6,7}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-4{1:7:1}, psn=inhib-{1,2,3,5,6,7}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-5{1:7:1}, psn=inhib-{1,2,3,4,6,7}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-6{1:7:1}, psn=inhib-{1,2,3,4,5,7}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=input-7{1:7:1}, psn=inhib-{1,2,3,4,5,6}, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),

    // hidden to output
    (prn=hidden-1, psn=output-1, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=hidden-2, psn=output-2, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=hidden-3, psn=output-3, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=hidden-4, psn=output-4, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=hidden-5, psn=output-5, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=hidden-6, psn=output-6, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=hidden-7, psn=output-7, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),

    // inhibition to output
    (prn=inhib-1, psn=output-1, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=inhib-2, psn=output-2, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=inhib-3, psn=output-3, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=inhib-4, psn=output-4, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=inhib-5, psn=output-5, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=inhib-6, psn=output-6, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig),
    (prn=inhib-7, psn=output-7, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig)
],

LRN=[
    // (fnc=stdp, inp=3 ms, exa=0.035, exp=3 ms),
    // (fnc=stdp, ina=0.035, inp=3 ms, exp=3 ms),
    //(fnc=stdp, ina=0.035, inp=3 ms, exa=0.035, exp=3 ms),
    (fnc=stdp, ina=0.035, inp=4 s, exa=0.015, exp=2 s),
    // (fnc=stdp, ina=0.035, inp=3 ms, exa=0.035, exp=3 ms),
    // (fnc=stdp, ina=0.0035, inp=3, exa=0.0035, exp=3),
    (fnc=flat)
],

CST=[
     (fnc=sig, dhl=10 s),
     (fnc=none)
]
)