## The machine settings sheet (sections built by the settings controller).

settings-section-machine = Machine
settings-section-memory = Memory
settings-section-display = Display
settings-section-disks = Disks
settings-section-network = Network
settings-section-advanced = Advanced
settings-machine-type = Machine
settings-cpu-count = Processors
settings-one-cpu = This machine takes one kind of processor.
settings-one-board = This machine takes one graphics board.
settings-next-power-on = Takes effect at the next power on.
settings-memory-banks = Memory banks
settings-memory-summary = { $mb } MB
settings-o2-display = The O2's display (GBE) is built in.
settings-heads = Heads
settings-disk-missing = The image is missing or can't be read.
settings-disks-summary = { $count ->
    [one] One disk
   *[other] { $count } disks
}
settings-pcap-interface = Host interface
settings-pcap-interface-help = The wired host interface to bridge onto. Leave empty to pick one.
settings-pcap-not-built = This build has no bridged networking; the machine uses NAT.
settings-net-off = Off
settings-advanced-summary = Every setting, as TOML
settings-toml = Complete configuration
settings-toml-help = The machine's TOML file. Applied only when it is valid.
settings-no-free-id = There's no free SCSI ID on the first controller.
settings-filter-disk-images = Disk images
