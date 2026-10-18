# Solar system birth

The guest keeps a deterministic planetesimal disk in its own memory. Each frame its CUDA SM86 kernel runs softened,
tiled all-pairs gravity with velocity-staggered leapfrog integration. A sorted uniform grid merges colliding bodies while
conserving mass and momentum; a fading gas disk damps eccentricity and migrates bodies inward.

The realtime variant advances 512 bodies once per frame. The offline showcase advances 2,048 bodies three times per
frame. Both use one additive instanced draw for the disk, growing planet seeds, their trails and the star field.
