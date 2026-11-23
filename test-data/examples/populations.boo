// Populations and connection rules: 4,000 neurons and 220,000 connections, without listing any of them.
//
// An excitatory and an inhibitory population fill a slab of cortex-like tissue. Each excitatory neuron receives
// 50 inputs from excitatory neurons anywhere and, with probability 0.2, from each inhibitory neuron within 150 µm;
// each inhibitory neuron receives 40 inputs from excitatory neurons within 200 µm. Turn on background input to
// "exc-" neurons and run: activity spreads and is held in check by inhibition.
(
POP=[
    (nam=exc, cnt=3200, mst=10 mV, mpd=20 ms, rfp=4 ms, csp=1 m/s,
     REG=(cst=bx, px1=0, px2=0, px3=0, sx1=800 µm, sx2=800 µm, sx3=200 µm)),
    (nam=inh, cnt=800, inh=t, mst=8 mV, mpd=10 ms, rfp=2 ms, csp=1 m/s,
     REG=(cst=bx, px1=0, px2=0, px3=0, sx1=800 µm, sx2=800 µm, sx3=200 µm), sed=2),
],
RUL=[
    (prp=exc, psp=exc, cnt=50, cnw=r0.2:0.6, lrn=stdp),
    (prp=inh, psp=exc, prb=0.2, rad=150 µm, cnw=r1:2),
    (prp=exc, psp=inh, cnt=40, rad=200 µm, cnw=r0.5:1),
],
LRN=[(fnc=stdp, ina=0.01, inp=30 ms, exa=0.01, exp=20 ms)],
)
