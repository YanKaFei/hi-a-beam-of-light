// Every secondary page needs a way home. navigateBack when there is a stack, otherwise land on
// the home page directly - a deep link can open echo with nothing beneath it.
function goHome() {
  const stack = getCurrentPages();
  if (stack.length > 1) wx.navigateBack({ delta: 1 });
  else wx.reLaunch({ url: "/pages/mine/mine" });
}
module.exports = { goHome };
