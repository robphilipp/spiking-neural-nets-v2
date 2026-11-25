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
    // SENSOR ARRAY
    // column 1
    (nid=input-11, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30, px2=-30, px3=0.)),
    (nid=input-12, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-13, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-14, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=0 µm, px3=0 µm)),
    (nid=input-15, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=10 µm, px3=0 µm)),
    (nid=input-16, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=20 µm, px3=0 µm)),
    (nid=input-17, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=30 µm, px3=0 µm)),

    // column 2
    (nid=input-21, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-22, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-23, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-24, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=0 µm, px3=0 µm)),
    (nid=input-25, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=10 µm, px3=0 µm)),
    (nid=input-26, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=20 µm, px3=0 µm)),
    (nid=input-27, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-20 µm, px2=30 µm, px3=0 µm)),

    // column 3
    (nid=input-31, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-32, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-33, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-34, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=0 µm, px3=0 µm)),
    (nid=input-35, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=10 µm, px3=0 µm)),
    (nid=input-36, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=20 µm, px3=0 µm)),
    (nid=input-37, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-10 µm, px2=30 µm, px3=0 µm)),

    // column 4
    (nid=input-41, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-42, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-43, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-44, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=0 µm)),
    (nid=input-45, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=10 µm, px3=0 µm)),
    (nid=input-46, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=20 µm, px3=0 µm)),
    (nid=input-47, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=30 µm, px3=0 µm)),

    // column 5
    (nid=input-51, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-52, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-53, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-54, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=0 µm, px3=0 µm)),
    (nid=input-55, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=10 µm, px3=0 µm)),
    (nid=input-56, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=20 µm, px3=0 µm)),
    (nid=input-57, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=10 µm, px2=30 µm, px3=0 µm)),

    // column 6
    (nid=input-61, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-62, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-63, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-64, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=0 µm, px3=0 µm)),
    (nid=input-65, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=10 µm, px3=0 µm)),
    (nid=input-66, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=20 µm, px3=0 µm)),
    (nid=input-67, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=20 µm, px2=30 µm, px3=0 µm)),

    // column 7
    (nid=input-71, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=-30 µm, px3=0 µm)),
    (nid=input-72, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=-20 µm, px3=0 µm)),
    (nid=input-73, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=-10 µm, px3=0 µm)),
    (nid=input-74, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=0 µm, px3=0 µm)),
    (nid=input-75, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=10 µm, px3=0 µm)),
    (nid=input-76, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=20 µm, px3=0 µm)),
    (nid=input-77, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=30 µm, px2=30 µm, px3=0 µm)),

    // HIDDEN LAYER
    (nid=hidden-1, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-80 µm, px3=100 µm)),
    (nid=hidden-2, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-70 µm, px3=100 µm)),
    (nid=hidden-3, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-60 µm, px3=100 µm)),
    (nid=hidden-4, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-50 µm, px3=100 µm)),
    (nid=hidden-5, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-40 µm, px3=100 µm)),
    (nid=hidden-6, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-30 µm, px3=100 µm)),
    (nid=hidden-7, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-20 µm, px3=100 µm)),
    (nid=hidden-8, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=-10 µm, px3=100 µm)),
    (nid=hidden-9, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=100 µm)),
    (nid=hidden-10, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=10 µm, px3=100 µm)),
    (nid=hidden-11, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=20 µm, px3=100 µm)),
    (nid=hidden-12, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=30 µm, px3=100 µm)),
    (nid=hidden-13, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=40 µm, px3=100 µm)),
    (nid=hidden-14, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=50 µm, px3=100 µm)),
    (nid=hidden-15, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=60 µm, px3=100 µm)),
    (nid=hidden-16, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=70 µm, px3=100 µm)),
    (nid=hidden-17, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=zer), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=80 µm, px3=100 µm)),

    // OUTPUT LAYER
    (nid=output-1, grp=group1, nty=mi, mst=10 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=2 h), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-5 µm, px2=0 µm, px3=200 µm)),
    (nid=output-2, grp=group1, nty=mi, mst=10 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=1 mV, spp=1 mV, csp=1 m/s, WDF=(fnc=exp, dhl=2 h), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=5 µm, px2=0 µm, px3=200 µm)),

    // INHIBITION NEURON
    (nid=inhib-1, grp=group1, nty=mi, mst=1.0 mV, inh=t, rfp=0.1 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms, mpr=2 ms, mpn=0.1 mV, spp=1 mV, csp=10 m/s, WDF=(fnc=exp, dhl=2 h), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=10, dpm=1, dpt=100 ms), WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=0 µm, px2=0 µm, px3=190 µm))
],

CON=[
    // input to hidden
    (prn=input-1{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),
    (prn=input-2{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),
    (prn=input-3{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),
    (prn=input-4{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),
    (prn=input-5{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),
    (prn=input-6{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),
    (prn=input-7{1:7:1}, psn=hidden-{1:17:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),

    // hidden to output
    (prn=hidden-{1:17:1}, psn=output-{1:2:1}, cnw=r0:2, eqw=r0:2, lrn=stdp),

    // output to inhib
    (prn=output-{1:2:1}, psn=inhib-1, cnw=1.0, eqw=1.0, lrn=flat),

    // inhib to output
    (prn=inhib-1, psn=output-{1:2:1}, cnw=10, eqw=10, lrn=flat)
],

LRN=[
    (fnc=stdp, ina=0.015, inp=40 ms, exa=0.015, exp=20 ms),
    //(fnc=stdp, ina=0.035, inp=40 ms, exa=0.035, exp=40 ms),
    (fnc=flat)
]
)