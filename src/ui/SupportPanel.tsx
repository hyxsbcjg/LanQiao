export function SupportPanel() {
  return <div className="support-panel">
    <h2>已支持</h2>
    <ul>
      <li>本地 HEX 装载、运行、暂停、单步、MCU 复位与整板重新上电；EEPROM / RTC 状态导入导出。</li>
      <li>V2 外观与 CT107D 电气键位；数码管、LED、继电器、电机、蜂鸣器、矩阵与独立按键。</li>
      <li>DS1302、DS18B20、PCF8591、AT24C02、NE555、超声波及温度、电压、频率、距离等环境输入。</li>
      <li>STC15 1T，默认主时钟 12MHz；CLK_DIV 八档动态分频。指令耗时采用引擎近似周期表。</li>
      <li>PCA 公共计数器：CMOD 八种时钟源、CF 溢出中断（7，向量 0x003B）与 PPCA 优先级。</li>
      <li>引擎支持 UART1 / UART2；本页串口终端仅连接 UART1，UART2 暂无 UI 终端。</li>
    </ul>
    <h2>未支持或有限支持</h2>
    <ul>
      <li>PCA 的 CCF、CCAPn 捕获、比较与 PWM 未实现，不产生对应输出或中断；ECI 引脚重映射未建模。</li>
      <li>UART 为配置驱动的帧级模型，不从 T1 / T2 重装寄存器推导波特率，不模拟波特率失配的采样错误；本页不提供波特率、校验位或停止位配置。</li>
      <li>T0 模式 3、WDT、IAP、掉电（Power-down）等完整低功耗、红外、点阵、LCD 与 PWA 离线不在 v0.1 支持范围。</li>
      <li>ADC、LVD、SPI、INT2 / INT3 等中断源尚未接入统一中断调度；数码管显示段码，不模拟 PWM 亮度。</li>
    </ul>
  </div>;
}
