## The stopped machine's summary page (native shells).

ov-spec = { $model } ({ $board })
ov-processor = Processor
ov-memory = Memory
ov-graphics = Graphics
ov-headless = None (serial console)
ov-prom = PROM
ov-prom-builtin = Built-in PROM
ov-scsi = SCSI { $id }
ov-fc = Fibre Channel { $id }
ov-empty = No image
ov-no-disc = CD-ROM, no disc
ov-cdrom = CD-ROM, { $disc }
ov-daynaport = DaynaPort Ethernet
ov-missing = missing
ov-cow = copy-on-write
ov-network = Network
ov-net-nat = NAT, { $subnet }
ov-net-bridged = Bridged to { $iface }
ov-net-stream = Stream to { $peer }
ov-forwards = { $count ->
    [one] one port forward
   *[other] { $count } port forwards
}
ov-saved-states = Saved states
ov-no-states = No saved states yet.
ov-power-on = Power On
ov-edit-settings = Edit Settings
ov-browse = Browse files
ov-hardware = Hardware
ov-no-machine = No machine yet. Choose File > New Machine.
ov-restore = Restore
ov-rename = Rename
ov-delete = Delete
