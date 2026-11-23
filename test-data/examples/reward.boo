// Reward learning: dopamine turns spike timing into learning.
//
// Two inputs drive an output neuron. Their STDP pairings only mark the synapses (an eligibility trace that fades
// over ~1 s); the weights change only if dopamine arrives while the marks last. Turn on background input to the
// "in-" neurons, run, inspect "out", and press "Release dopamine": the weights step up.
(
MOD=[(nam=dopamine, clr=300 ms, lat=1 ms)],
NRN=[
    (nid=in-1, mst=5 mV, mpn=0 mV, csp=1 m/s, LOC=(cst=ct, px1=0, px2=0, px3=0)),
    (nid=in-2, mst=5 mV, mpn=0 mV, csp=1 m/s, LOC=(cst=ct, px1=0, px2=50, px3=0)),
    (nid=out, mst=0.8 mV, mpn=0 mV, rfp=5 ms, WDF=(fnc=zer), WLF=(fnc=bnd, lwb=0, upb=5),
        LOC=(cst=ct, px1=100, px2=25, px3=0),
        RCP=[(mod=dopamine, elg=1 s, rat=20)]),
],
CON=[
    (prn={in-1, in-2}, psn=out, cnw=1, lrn=stdp),
],
LRN=[(fnc=stdp_hard, nam=stdp, ina=0.005, inp=20 ms, exa=0.02, exp=20 ms)],
)
