use anyhow::Result;

static STATUS_MAX_RETRY: u32 = 0b100;
static STATUS_OK: u32 = 0b111;
pub static STATUS_COMPLETED: u32 = 1 << 31;

/// 用来表示下载的状态，不想写太多列了，所以仅使用一个 u32 表示。
/// 从低位开始，固定每三位表示一种子任务的状态。
/// 子任务状态从 0b000 开始，每执行失败一次将状态加一，最多 0b100（即允许重试 4 次），该值定义为 STATUS_MAX_RETRY。
/// 如果子任务执行成功，将状态设置为 0b111，该值定义为 STATUS_OK。
/// 子任务达到最大失败次数或者执行成功时，认为该子任务已经完成。
/// 当所有子任务都已经完成时，为最高位打上标记 1，表示整个下载任务已经完成。
#[derive(Clone, Copy, Default)]
pub struct Status<const N: usize>(u32);

impl<const N: usize> Status<N> {
    // 获取最高位的完成标记
    pub fn get_completed(&self) -> bool {
        self.0 >> 31 == 1
    }

    /// 依次检查所有子任务是否还应该继续执行，返回一个 bool 数组
    pub fn should_run(&self) -> [bool; N] {
        let mut result = [false; N];
        for (i, item) in result.iter_mut().enumerate() {
            *item = self.check_continue(i);
        }
        result
    }

    /// 根据任务结果更新状态，任务结果是一个 Result 数组，需要与子任务一一对应
    /// 如果所有子任务都已经完成，那么打上最高位的完成标记
    pub fn update_status(&mut self, result: &[Result<()>]) {
        assert!(result.len() == N, "result length should be equal to N");
        for (i, res) in result.iter().enumerate() {
            self.set_result(res, i);
        }
        if self.should_run().iter().all(|x| !x) {
            // 所有任务都成功或者由于尝试次数过多失败，为 status 最高位打上标记，将来不再重试
            self.set_completed(true)
        }
    }

    /// 设置最高位的完成标记
    fn set_completed(&mut self, completed: bool) {
        if completed {
            self.0 |= 1 << 31;
        } else {
            self.0 &= !(1 << 31);
        }
    }

    /// 获取某个子任务的状态
    fn get_status(&self, offset: usize) -> u32 {
        (self.0 >> (offset * 3)) & 0b111
    }

    /// 设置某个子任务的状态
    fn set_status(&mut self, offset: usize, status: u32) {
        self.0 = (self.0 & !(0b111 << (offset * 3))) | (status << (offset * 3));
    }

    // 将某个子任务的状态加一（在任务失败时使用）
    fn plus_one(&mut self, offset: usize) {
        self.0 += 1 << (3 * offset);
    }

    // 设置某个子任务的状态为 STATUS_OK（在任务成功时使用）
    fn set_ok(&mut self, offset: usize) {
        self.0 |= STATUS_OK << (3 * offset);
    }

    /// 检查某个子任务是否还应该继续执行，实际是检查该子任务的状态是否小于 STATUS_MAX_RETRY
    fn check_continue(&self, offset: usize) -> bool {
        self.get_status(offset) < STATUS_MAX_RETRY
    }

    /// 根据子任务执行结果更新子任务的状态
    /// 如果 Result 是 Ok，那么认为任务执行成功，将状态设置为 STATUS_OK
    /// 如果 Result 是 Err，那么认为任务执行失败，将状态加一
    fn set_result(&mut self, result: &Result<()>, offset: usize) {
        if self.get_status(offset) < STATUS_MAX_RETRY {
            match result {
                Ok(_) => self.set_ok(offset),
                Err(_) => self.plus_one(offset),
            }
        }
    }
}

impl<const N: usize> From<u32> for Status<N> {
    fn from(status: u32) -> Self {
        Status(status)
    }
}

impl<const N: usize> From<Status<N>> for u32 {
    fn from(status: Status<N>) -> Self {
        status.0
    }
}

impl<const N: usize> From<Status<N>> for [u32; N] {
    fn from(status: Status<N>) -> Self {
        let mut result = [0; N];
        for (i, item) in result.iter_mut().enumerate() {
            *item = status.get_status(i);
        }
        result
    }
}

impl<const N: usize> From<[u32; N]> for Status<N> {
    fn from(status: [u32; N]) -> Self {
        let mut result = Status::<N>::default();
        for (i, item) in status.iter().enumerate() {
            result.set_status(i, *item);
        }
        if result.should_run().iter().all(|x| !x) {
            result.set_completed(true);
        }
        result
    }
}

/// 包含五个子任务，从前到后依次是：视频封面、视频信息、Up 主头像、Up 主信息、分 P 下载
pub type VideoStatus = Status<5>;

/// 包含五个子任务，从前到后分别是：视频封面、视频内容、视频信息、视频弹幕、视频字幕
pub type PageStatus = Status<5>;

#[cfg(test)]
mod test {
    use anyhow::anyhow;

    use super::*;

    #[test]
    fn test_status_update() {
        let mut status = Status::<3>::default();
        assert_eq!(status.should_run(), [true, true, true]);
        for _ in 0..3 {
            status.update_status(&[Err(anyhow!("")), Ok(()), Ok(())]);
            assert_eq!(status.should_run(), [true, false, false]);
        }
        status.update_status(&[Err(anyhow!("")), Ok(()), Ok(())]);
        assert_eq!(status.should_run(), [false, false, false]);
    }

    #[test]
    fn test_status_convert() {
        let testcases = [[0, 0, 1], [1, 2, 3], [3, 1, 2], [3, 0, 7]];
        for testcase in testcases.iter() {
            let status = Status::<3>::from(testcase.clone());
            assert_eq!(<[u32; 3]>::from(status), *testcase);
        }
    }

    #[test]
    fn test_status_convert_and_update() {
        let testcases = [([0, 0, 1], [1, 7, 7]), ([3, 4, 3], [4, 4, 7]), ([3, 1, 7], [4, 7, 7])];
        for (before, after) in testcases.iter() {
            let mut status = Status::<3>::from(before.clone());
            status.update_status(&[Err(anyhow!("")), Ok(()), Ok(())]);
            assert_eq!(<[u32; 3]>::from(status), *after);
        }
    }
}
