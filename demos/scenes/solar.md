# Solar system birth

The guest keeps a deterministic minimum-mass planetesimal disk in its own memory. Each frame its CUDA SM86 kernel uses
NVIDIA's sample n-body tile structure for softened mutual kicks and an exact Kepler drift around the star. A sorted
uniform grid merges overlapping bodies while conserving mass and momentum; a fading gas disk damps eccentricity and
inclination and migrates bodies inward.

The realtime variant advances 512 bodies once per frame. The offline showcase advances 2,048 bodies three times per
frame. Both run through the translator from a guest QMD, then use one additive instanced draw for the disk, growing
planet seeds, their trails and the star field.
