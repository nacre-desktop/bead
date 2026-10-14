-- SPDX-License-Identifier: GPL-3.0-only
return {stores={open=true,query="Reusable input",enabled=false,volume=25,choice="quiet",scroll=0,details=0},
 actions={toggle=function(s) s.enabled=not s.enabled end, detail=function(s) s.details=s.details+1 end,
 set_volume=function(s,n) s.volume=n end, select=function(s,id) s.choice=id end},computed={}}
