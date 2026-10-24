-- SPDX-License-Identifier: GPL-3.0-only
-- Stores below are projected by `chat.rs`; the view only reads them.
return {stores={
    ["panel.open"]=true,["view.detached"]=false,["view.edge"]="left",
    ["chat.labels"]={},["chat.section"]="chat",["chat.pinned"]=false,["chat.height"]=650,["chat.edge"]=16,["chat.button"]=36,["chat.tabs"]=62,["chat.top"]=116,["chat.inner"]=438,
    ["chat.draft"]="",["chat.placeholder"]="",
    ["chat.scroll"]=0,["chat.extent"]=0,["chat.cells"]={},["chat.empty"]=true,
    ["chat.transcript_top"]=189,["chat.transcript_height"]=300,
    ["chat.heading"]="",["chat.status"]="",["chat.status_error"]=false,["chat.status_top"]=0,["chat.status_height"]=0,
    ["chat.has_question"]=false,["chat.question_rows"]={},["chat.question_top"]=0,["chat.question_height"]=0,
    ["chat.question_extent"]=0,["chat.qscroll"]=0,
    ["chat.answer.0"]="",["chat.answer.1"]="",["chat.answer.2"]="",["chat.answer.3"]="",
    ["chat.composer_top"]=0,["chat.composer_height"]=100,["chat.actions_top"]=0,
    ["chat.busy"]=false,["chat.workspace"]=false,["chat.can_new"]=false,["chat.can_workspace"]=false,
    ["chat.can_send"]=false,["chat.can_reply"]=false,["chat.can_stop"]=false,
    ["chat.provider"]="",["chat.saved"]={},["chat.saved_state"]="",
},actions={
    close=function(s) s["panel.open"]=false end,
    -- The chat Runtime authorizes and handles this host action.
    ["chat.do"]=function(_) end,
},computed={}}
